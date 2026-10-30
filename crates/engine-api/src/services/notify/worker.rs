use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::Rng;
use tokio::task::JoinSet;

use crate::ApiState;
use crate::error::RESOURCE_EXHAUSTED;
use crate::infra::liveops;
use crate::infra::notify::{Message, Receipt, Registry, Vendor};
use crate::repositories::db::Queries;
use crate::repositories::models::DeliveryAttempt;
use crate::repositories::notify::{DEAD, DELIVERED, QUEUED};
use crate::services::deps::q;
use crate::services::quota;

/// One backoff step never grows past this, so a relay that is down does not push
/// a message a day into the future.
const BACKOFF_CAP: u64 = 900_000;

/// Drains the delivery queue. A request writes the message it made true and
/// leaves the vendors to this loop, so a relay that takes ten seconds to answer
/// cannot slow a player down.
pub async fn delivery_loop(state: ApiState) {
    let config = liveops::config();
    let concurrency = config.notify_concurrency;
    let mut beat = tokio::time::interval(Duration::from_millis(config.notify_poll_ms));
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut in_flight = JoinSet::new();
    loop {
        beat.tick().await;
        let Ok(queries) = q(&state).await else {
            continue;
        };
        // A row still marked `sending` twice the vendor timeout ago was taken by
        // a process that never came back for it.
        let _ = queries
            .requeue_stalled_attempts(stalled_before(Utc::now(), config.notify_timeout_ms))
            .await;
        while in_flight.try_join_next().is_some() {}
        if in_flight.len() >= concurrency {
            continue;
        }
        let room = (concurrency - in_flight.len()) as i64;
        let due = match queries.due_attempts(room * 4).await {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!("[notify] the queue could not be read: {err}");
                continue;
            }
        };
        let registry = Registry::from_env();
        for row in due {
            if in_flight.len() >= concurrency {
                break;
            }
            let Some(vendor) = registry.vendor(&row.provider) else {
                // The credentials went away after this was queued. Say so on the
                // row instead of marking it delivered: nobody got anything.
                let reason = registry.refusal(&row.provider).unwrap_or_default();
                let _ = queries.abandon_attempt(row.id, &reason).await;
                continue;
            };
            let queries = queries.clone();
            let state = state.clone();
            in_flight.spawn(async move {
                settle(&queries, &state, vendor, row).await;
            });
        }
    }
}

/// Claims one row, spends one unit of the account's budget on it and writes the
/// vendor's answer back.
async fn settle(
    queries: &Arc<Queries>,
    state: &ApiState,
    vendor: Arc<dyn Vendor>,
    row: DeliveryAttempt,
) {
    let Some(row) = queries.claim_attempt(row.id).await.ok().flatten() else {
        return;
    };
    // The budget is spent when the message goes, not when it is queued: a queue
    // nobody is paying for is a queue that grows.
    if let Err(err) = quota::charge(state, row.user_id, quota::NOTIFY_SEND, 1).await {
        if err.kind == RESOURCE_EXHAUSTED {
            let wait = i64::from(err.retry_after.unwrap_or(60)).max(1);
            let _ = queries
                .defer_attempt(row.id, Utc::now() + chrono::Duration::seconds(wait))
                .await;
        }
        return;
    }
    let attempts = row.attempts + 1;
    let max = liveops::config().notify_max_attempts;
    let outcome = vendor.send(message(&row)).await;
    let (status, next, error) = match outcome {
        Ok(Receipt::Accepted) => (DELIVERED, Utc::now(), String::new()),
        Ok(Receipt::Refused(reason)) => (DEAD, Utc::now(), reason),
        Ok(Receipt::Later(reason)) => owed(attempts, max, reason),
        Err(reason) => owed(attempts, max, reason),
    };
    let error = clip(error);
    let _ = queries
        .settle_attempt(row.id, status, attempts, &error, next)
        .await;
}

/// A vendor that only paused is owed another try, until the ceiling says the
/// message is older than anyone will want it.
fn owed(attempts: i64, max: i64, reason: String) -> (&'static str, DateTime<Utc>, String) {
    if attempts >= max {
        return (DEAD, Utc::now(), reason);
    }
    (QUEUED, Utc::now() + retry_delay(attempts), reason)
}

fn message(row: &DeliveryAttempt) -> Message {
    Message {
        recipient: row.recipient.clone(),
        title: row.subject.clone(),
        text: row.text.clone(),
    }
}

/// How long to wait before attempt `attempts` (1-based) goes back on the queue.
/// The step doubles, caps, then takes up to half again as much jitter, so a fleet
/// of workers does not hammer one relay in lockstep.
pub(crate) fn retry_delay(attempts: i64) -> Duration {
    let base = liveops::config().notify_backoff_ms.max(1) as u64;
    let step = base
        .saturating_mul(1u64 << (attempts - 1).min(16) as u64)
        .min(BACKOFF_CAP);
    Duration::from_millis(step + rand::thread_rng().gen_range(0..=step / 2))
}

/// A row this much past its last write belongs to a process that is gone.
pub(crate) fn stalled_before(now: DateTime<Utc>, timeout_ms: u64) -> DateTime<Utc> {
    let twice = chrono::Duration::milliseconds(timeout_ms as i64 * 2);
    now - twice
}

fn clip(text: String) -> String {
    text.chars().take(200).collect()
}
