//! Purchases and subscriptions, as the console lists them.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::models::{Purchase, Subscription};

/// One purchase receipt.
#[derive(Debug, Serialize)]
pub struct PurchaseRow {
    pub transaction_id: String,
    pub user_id: Uuid,
    pub product_id: String,
    pub store: i16,
    pub environment: i16,
    pub purchase_time: DateTime<Utc>,
    pub refund_time: DateTime<Utc>,
    pub create_time: DateTime<Utc>,
}

impl From<Purchase> for PurchaseRow {
    fn from(p: Purchase) -> Self {
        Self {
            transaction_id: p.transaction_id,
            user_id: p.user_id,
            product_id: p.product_id,
            store: p.store,
            environment: p.environment,
            purchase_time: p.purchase_time,
            refund_time: p.refund_time,
            create_time: p.create_time,
        }
    }
}

/// One subscription. `active` reads the same way the player API does: not expired
/// and not refunded.
#[derive(Debug, Serialize)]
pub struct SubscriptionRow {
    pub original_transaction_id: String,
    pub user_id: Uuid,
    pub product_id: String,
    pub store: i16,
    pub environment: i16,
    pub purchase_time: DateTime<Utc>,
    pub expire_time: DateTime<Utc>,
    pub refund_time: DateTime<Utc>,
    pub active: bool,
}

impl From<Subscription> for SubscriptionRow {
    fn from(s: Subscription) -> Self {
        let now = Utc::now();
        Self {
            active: s.expire_time > now && s.refund_time.timestamp() == 0,
            original_transaction_id: s.original_transaction_id,
            user_id: s.user_id,
            product_id: s.product_id,
            store: s.store,
            environment: s.environment,
            purchase_time: s.purchase_time,
            expire_time: s.expire_time,
            refund_time: s.refund_time,
        }
    }
}
