use crate::services::plane::Plane;

use std::sync::Arc;

use std::sync::atomic::{AtomicI64, Ordering};

use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::liveops;
use crate::repositories::db::Queries;
use crate::repositories::idempotency::ClaimOutcome;
use crate::repositories::models::IdempotencyKey;

pub(crate) enum Begin {
    Bypass,
    Ticket(Ticket),
    Replay(Stored),
}

/// A reply that was already produced for this key. The status travels with the
/// reply so a replayed failure is a failure, not a 200 carrying an error object,
/// and the body is kept as the exact bytes the handler wrote so a replay is
/// byte-identical instead of a re-serialisation.
pub(crate) struct Stored {
    pub status: u16,
    pub raw: String,
}

/// Marker that separates the stored envelope from a handler that legitimately
/// returns an object with `status` and `body` keys.
const ENVELOPE: &str = "__idempotency";

impl Stored {
    pub(crate) fn encode(&self) -> serde_json::Value {
        serde_json::json!({
            ENVELOPE: 1,
            "status": self.status,
            "raw": self.raw,
        })
    }

    fn decode(value: serde_json::Value) -> Stored {
        let marked = value
            .as_object()
            .is_some_and(|map| map.get(ENVELOPE).is_some());
        if !marked {
            // Rows written before the envelope existed carry a bare body.
            return Stored {
                status: 200,
                raw: value.to_string(),
            };
        }
        Stored {
            status: value
                .get("status")
                .and_then(serde_json::Value::as_u64)
                .and_then(|status| u16::try_from(status).ok())
                .unwrap_or(200),
            raw: value
                .get("raw")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        }
    }
}

pub(crate) struct Ticket {
    id: String,
    queries: Arc<Queries>,
}

impl Begin {
    pub(crate) fn replayed(&mut self) -> Option<Stored> {
        match self {
            Begin::Replay(stored) => Some(Stored {
                status: stored.status,
                raw: std::mem::take(&mut stored.raw),
            }),
            _ => None,
        }
    }

    /// Deterministic row id for the write this request performs. Without a key
    /// the caller keeps a fresh `Uuid::new_v4()`; with one, retries land on the
    /// same primary key even if the claim row races.
    pub(crate) fn operation_uuid(&self) -> Option<Uuid> {
        match self {
            Begin::Ticket(ticket) => Some(ticket.operation_uuid()),
            _ => None,
        }
    }

    pub(crate) async fn settle(self, outcome: Option<Stored>) {
        if let Begin::Ticket(ticket) = self {
            match outcome {
                Some(stored) => ticket.complete(&stored).await,
                None => ticket.fail().await,
            }
        }
    }
}

impl Ticket {
    pub(crate) fn operation_uuid(&self) -> Uuid {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hex::decode(&self.id[..32]).expect("hex record id"));
        Uuid::from_bytes(bytes)
    }

    async fn complete(&self, stored: &Stored) {
        if let Err(err) = self
            .queries
            .complete_idempotency(&self.id, &stored.encode())
            .await
        {
            tracing::error!(
                "[idempotency] cannot store the response for {}: {err}",
                self.id
            );
        }
    }

    async fn fail(&self) {
        if let Err(err) = self.queries.release_idempotency(&self.id).await {
            tracing::error!("[idempotency] cannot release {}: {err}", self.id);
        }
    }
}

pub(crate) fn request_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0x1f]);
    }
    hex::encode(hasher.finalize())
}

pub(crate) async fn begin(
    plane: &Plane,
    headers: &HeaderMap,
    scope: &str,
    actor: &str,
    parts: &[&str],
) -> Result<Begin, V2Error> {
    let Some(key) = supplied_key(headers)? else {
        return Ok(Begin::Bypass);
    };
    let queries = plane.q().await?;
    let id = request_hash(&[actor, scope, &key]);
    let now = Utc::now();
    let row = IdempotencyKey {
        id: id.clone(),
        actor: actor.to_owned(),
        scope: scope.to_owned(),
        request_key: key,
        request_hash: request_hash(parts),
        status: "pending".to_owned(),
        response: serde_json::Value::Null,
        create_time: now,
        update_time: now,
    };
    let outcome = queries.claim_idempotency(&row).await?;
    prune_expired(&queries);
    match outcome {
        ClaimOutcome::Acquired => Ok(Begin::Ticket(Ticket { id, queries })),
        ClaimOutcome::Replay(stored) if stored.status == "done" => {
            if stored.request_hash != row.request_hash {
                return Err(V2Error::invalid_argument(
                    "This Idempotency-Key was already used for a different request.",
                ));
            }
            Ok(Begin::Replay(Stored::decode(stored.response)))
        }
        ClaimOutcome::Replay(_) => Err(V2Error::already_exists(
            "A request with this Idempotency-Key is still being processed.",
        )),
    }
}

fn supplied_key(headers: &HeaderMap) -> Result<Option<String>, V2Error> {
    let Some(raw) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    let raw = raw
        .to_str()
        .map_err(|_| V2Error::invalid_argument("The Idempotency-Key must be ASCII."))?;
    let key = raw.trim();
    let acceptable = !key.is_empty()
        && key.len() <= 128
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'));
    if !acceptable {
        return Err(V2Error::invalid_argument(
            "The Idempotency-Key must be 1-128 characters of letters, digits, dash, underscore or colon.",
        ));
    }
    Ok(Some(key.to_owned()))
}

fn prune_expired(queries: &Arc<Queries>) {
    static LAST_PRUNE: AtomicI64 = AtomicI64::new(0);
    let now = Utc::now().timestamp();
    let last = LAST_PRUNE.load(Ordering::Relaxed);
    if now - last < 3_600
        || LAST_PRUNE
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    let queries = queries.clone();
    let cutoff = Utc::now() - Duration::days(liveops::config().idempotency_ttl_days.max(1));
    tokio::spawn(async move {
        if let Err(err) = queries.prune_idempotency(cutoff).await {
            tracing::warn!("[idempotency] prune failed: {err}");
        }
    });
}
