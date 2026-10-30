use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::liveops;
use crate::repositories::db::Queries;
use crate::repositories::idempotency::ClaimOutcome;
use crate::repositories::models::IdempotencyKey;
use crate::services::deps::q;

pub(crate) enum Begin {
    Bypass,
    Ticket(Ticket),
    Replay(serde_json::Value),
}

pub(crate) struct Ticket {
    id: String,
    queries: Arc<Queries>,
}

impl Begin {
    pub(crate) fn replayed(&mut self) -> Option<serde_json::Value> {
        match self {
            Begin::Replay(value) => Some(std::mem::take(value)),
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

    pub(crate) async fn settle(self, outcome: Option<serde_json::Value>) {
        if let Begin::Ticket(ticket) = self {
            match outcome {
                Some(value) => ticket.complete(&value).await,
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

    async fn complete(&self, response: &serde_json::Value) {
        if let Err(err) = self.queries.complete_idempotency(&self.id, response).await {
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
    state: &ApiState,
    headers: &HeaderMap,
    scope: &str,
    actor: &str,
    parts: &[&str],
) -> Result<Begin, V2Error> {
    let Some(key) = supplied_key(headers)? else {
        return Ok(Begin::Bypass);
    };
    let queries = q(state).await?;
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
            Ok(Begin::Replay(stored.response))
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
