use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::Rng;

use crate::db::time;
use crate::infra::liveops;
use crate::infra::webhook::{DELIVERY_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER, signature};
use crate::repositories::db::Queries;
use crate::repositories::models::{WebhookDelivery, WebhookEndpoint};
use crate::repositories::webhook::{DEAD, DELIVERED, QUEUED};

/// One backoff step never grows past this, so a silent endpoint does not push a
/// fact a day into the future.
const BACKOFF_CAP_MS: u64 = 60_000;

/// What came back from one HTTP attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The endpoint took the fact; nothing more is owed.
    Delivered,
    /// The endpoint is down, busy, or shedding load. Owed again later, and this
    /// is the kind of failure that adds up into a circuit.
    Failing,
    /// The endpoint answered and said no in a way retrying cannot fix.
    Rejected,
}

pub(crate) struct Answer {
    pub code: i64,
    pub error: String,
    pub verdict: Verdict,
}

/// The body put on the wire, rebuilt from the row every time, so a retry sends
/// the same bytes as the first attempt.
pub(crate) fn envelope(row: &WebhookDelivery) -> serde_json::Value {
    serde_json::json!({
        "event": row.event,
        "event_id": row.event_id,
        "occurred_at": time::encode(row.create_time),
        "payload": row.payload,
    })
}

/// How long to wait before attempt `attempt` (1-based) goes back on the queue.
/// The step doubles, capped, then takes up to half again as much jitter so a
/// fleet of workers does not retry one endpoint in lockstep. The jitter is
/// bounded by the doubling, so the sequence stays strictly increasing.
pub(crate) fn retry_delay(attempt: i64) -> Duration {
    let base = liveops::config().webhook_backoff_ms.max(1) as u64;
    let step = base
        .saturating_mul(1u64 << (attempt - 1).min(16) as u64)
        .min(BACKOFF_CAP_MS);
    Duration::from_millis(step + rand::thread_rng().gen_range(0..=step / 2))
}

/// 429 is the endpoint asking for room and 408 is a gateway that stopped
/// waiting; both are owed the fact again. Any other 4xx is a decision.
pub(crate) fn classify(code: u16) -> Verdict {
    if (200..300).contains(&code) {
        Verdict::Delivered
    } else if code == 408 || code == 429 || code >= 500 {
        Verdict::Failing
    } else {
        Verdict::Rejected
    }
}

/// Sends one delivery. The signature covers the bytes being sent and the
/// timestamp is this attempt's, so a retry is a fresh delivery rather than a
/// replay of a stale one.
async fn send(
    client: &reqwest::Client,
    endpoint: &WebhookEndpoint,
    row: &WebhookDelivery,
) -> Answer {
    let body = envelope(row).to_string();
    let timestamp = Utc::now().timestamp().to_string();
    let response = client
        .post(&endpoint.url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            SIGNATURE_HEADER,
            signature(&endpoint.secret, &timestamp, &body),
        )
        .header(TIMESTAMP_HEADER, timestamp)
        .header(DELIVERY_HEADER, row.id.to_string())
        .body(body)
        .send()
        .await;
    match response {
        Ok(response) => {
            let code = response.status().as_u16();
            let verdict = classify(code);
            Answer {
                code: code as i64,
                error: if verdict == Verdict::Delivered {
                    String::new()
                } else {
                    clip(format!("the endpoint answered {code}"))
                },
                verdict,
            }
        }
        Err(err) => Answer {
            code: 0,
            error: clip(err.to_string()),
            verdict: Verdict::Failing,
        },
    }
}

/// Posts `row` once and writes the outcome. `None` means another worker already
/// holds the claim, so this worker did nothing and owes the circuit no verdict.
pub(crate) async fn attempt(
    queries: &Arc<Queries>,
    client: &reqwest::Client,
    endpoint: &WebhookEndpoint,
    row: &WebhookDelivery,
) -> Option<Verdict> {
    let Some(row) = queries.claim_delivery(row.id).await.ok()? else {
        return None;
    };
    let answer = send(client, endpoint, &row).await;
    let attempt = row.attempt + 1;
    let (status, next) = match answer.verdict {
        Verdict::Delivered => (DELIVERED, Utc::now()),
        Verdict::Failing if attempt < liveops::config().webhook_max_attempts => {
            (QUEUED, Utc::now() + retry_delay(attempt))
        }
        Verdict::Failing | Verdict::Rejected => (DEAD, Utc::now()),
    };
    let _ = queries
        .settle_delivery(&row, status, attempt, answer.code, &answer.error, next)
        .await;
    Some(answer.verdict)
}

/// A row this much past its last write is owed by the process that claimed it
/// and never came back for.
pub(crate) fn stalled_before(now: DateTime<Utc>, timeout: Duration) -> DateTime<Utc> {
    let twice =
        chrono::Duration::from_std(timeout * 2).unwrap_or_else(|_| chrono::Duration::seconds(120));
    now - twice
}

fn clip(text: impl Into<String>) -> String {
    text.into().chars().take(200).collect()
}
