use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::liveops;
use crate::repositories::db::Queries;
use crate::repositories::models::Subscription;
use crate::utils::iap::epoch;

/// A subscription pays out while it has not been refunded and its expiry has
/// not passed — or has passed by no more than the grace window, which is how a
/// store's own grace period looks from here. A row the store never gave an
/// expiry to keeps the epoch default and reads as never ending: only a refund
/// closes it.
pub(crate) fn is_active(sub: &Subscription, now: DateTime<Utc>) -> bool {
    !is_cancelled(sub) && (never_expires(sub) || sub.expire_time > now - grace_window())
}

/// Active only because of the grace window: the entitlement holds, but the
/// store has not confirmed a renewal yet.
pub(crate) fn in_grace(sub: &Subscription, now: DateTime<Utc>) -> bool {
    !never_expires(sub) && sub.expire_time > now - grace_window() && sub.expire_time <= now
}

fn is_cancelled(sub: &Subscription) -> bool {
    sub.refund_time > epoch()
}

fn never_expires(sub: &Subscription) -> bool {
    sub.expire_time <= epoch()
}

/// 0 refunded, 1 active, 2 active only inside the grace window, 3 expired.
pub(crate) fn state_of(sub: &Subscription, now: &DateTime<Utc>) -> i32 {
    if is_cancelled(sub) {
        0
    } else if !is_active(sub, *now) {
        3
    } else if in_grace(sub, *now) {
        2
    } else {
        1
    }
}

fn grace_window() -> Duration {
    Duration::days(liveops::config().subscription_grace_days)
}

/// The product ids whose subscription is paying out for this account right now.
pub(crate) async fn active_products(
    queries: &Arc<Queries>,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<BTreeSet<String>, V2Error> {
    Ok(queries
        .subscriptions_of(user_id)
        .await?
        .iter()
        .filter(|row| is_active(row, now))
        .map(|row| row.product_id.clone())
        .collect())
}
