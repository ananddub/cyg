use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::models::PlayerQuota;
use crate::repositories::quota::quota_id;

/// The scope names a route charges, and the windows an operator tunes.
pub use engine_config::quota::{CHANNEL_SEND, LOGIN, NOTIFY_SEND, STORAGE_WRITE, WEBHOOK_RETRY};

/// A charge reads, decides, and writes, so concurrent requests on one account
/// converge on the row instead of queueing behind a lock. Three passes covers
/// the interleaving that can happen between a read and its guarded write.
const ATTEMPTS: u32 = 3;

/// Spends `amount` of one account's window in `scope`. With the quotas off
/// nothing is charged, so an opted-out plane behaves exactly as it did before
/// this existed.
pub async fn charge(state: &ApiState, uid: Uuid, scope: &str, amount: u32) -> Result<(), V2Error> {
    let quotas = &state.control.quotas;
    if !quotas.enabled {
        return Ok(());
    }
    let queries = crate::services::deps::q(state).await?;
    let window = quotas.window(scope);
    if window.cap == 0 {
        return Ok(());
    }
    if amount > window.cap {
        return Err(refused(
            scope,
            "This request is larger than the account's whole allowance.",
            window.period_secs as i64,
        ));
    }
    for _ in 0..ATTEMPTS {
        let now = Utc::now();
        let boundary = now - Duration::seconds(window.period_secs.max(1) as i64);
        let Some(row) = queries.quota_row(uid, scope).await? else {
            if queries
                .open_quota(&PlayerQuota {
                    id: quota_id(uid, scope),
                    user_id: uid,
                    scope: scope.to_string(),
                    used: amount,
                    cap: window.cap,
                    period_secs: window.period_secs,
                    window_start: now,
                    update_time: now,
                })
                .await?
            {
                return Ok(());
            }
            continue;
        };
        if row.window_start <= boundary {
            if queries
                .roll_window(
                    uid,
                    scope,
                    amount,
                    window.cap,
                    window.period_secs,
                    boundary,
                    now,
                )
                .await?
            {
                return Ok(());
            }
            continue;
        }
        if row.used + amount > window.cap {
            let ends = row.window_start + Duration::seconds(window.period_secs.max(1) as i64);
            return Err(refused(
                scope,
                "This account has used its allowance for this window.",
                (ends - now).num_seconds(),
            ));
        }
        if queries
            .charge_window(uid, scope, amount, window.cap, boundary, now)
            .await?
        {
            return Ok(());
        }
    }
    tracing::debug!("[quota] {scope} lost three writes in a row");
    Err(V2Error::failed_precondition(
        "This request could not be charged while another write was in flight.",
    ))
}

/// A whole window from now: the scope was never charged, so nobody knows the
/// row's start better than the caller does.
fn refused(scope: &str, message: impl Into<String>, secs: i64) -> V2Error {
    let message = message.into();
    tracing::debug!("[quota] {scope}: {message}");
    V2Error::resource_exhausted(message).with_retry_after(secs.clamp(1, i64::from(u32::MAX)) as u32)
}
