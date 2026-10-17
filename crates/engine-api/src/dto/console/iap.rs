//! Purchases and subscriptions, as the console lists and validates them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dto::iap::{PurchaseResponse, SubscriptionResponse};
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

/// `POST /v2/console/iap/{vendor}/validate` — one credential in whichever
/// spelling the vendor hands it, plus the reference server's `persist` flag.
/// Persisting (the default) writes the row under `user_id`; `persist: false`
/// only parses the entity back out.
#[derive(Debug, Default, Deserialize)]
pub struct ConsoleValidateRequest {
    #[serde(default)]
    pub receipt: String,
    #[serde(default)]
    pub purchase_token: String,
    #[serde(default)]
    pub signed_request: Option<String>,
    #[serde(default)]
    pub purchase_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    /// `"purchase"` (the default) or `"subscription"`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub persist: Option<bool>,
    #[serde(default)]
    pub user_id: Option<String>,
}

impl ConsoleValidateRequest {
    /// The first credential actually filled, in the envelope's own order.
    pub(crate) fn credential(&self) -> &str {
        for candidate in [
            Some(self.receipt.as_str()),
            Some(self.purchase_token.as_str()),
            self.signed_request.as_deref(),
            self.purchase_id.as_deref(),
        ] {
            if candidate.is_some_and(|c| !c.is_empty()) {
                return candidate.unwrap_or_default();
            }
        }
        ""
    }

    /// The reference server's default: validation persists unless told not to.
    pub(crate) fn persists(&self) -> bool {
        self.persist.unwrap_or(true)
    }
}

/// `api.ValidatePurchaseResponse`'s shape for the console validate reply.
#[derive(Debug, Serialize)]
pub struct ValidatedPurchaseReply {
    pub validated_purchases: Vec<PurchaseResponse>,
}

/// `api.ValidateSubscriptionResponse`'s shape: one subscription, not a list.
#[derive(Debug, Serialize)]
pub struct ValidatedSubscriptionReply {
    pub subscription: SubscriptionResponse,
}
