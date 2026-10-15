//! `/v2/iap*` — the shapes purchase and subscription receipts travel in.
//!
//! The row → wire mapping lives here, the one place that knows both sides;
//! `utils::iap` keeps only the pure receipt helpers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::{Purchase, Subscription};

/// The paging query shared by `GET /v2/iap/purchase` and
/// `GET /v2/iap/subscription`.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// The Apple receipt body — `POST /v2/iap` and `POST /v2/iap/purchase/apple`.
#[derive(Debug, Deserialize)]
pub struct PurchaseRequest {
    #[serde(default)]
    pub receipt: String,
    #[serde(default)]
    pub store: Option<i16>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub transaction_id: Option<String>,
}

/// The purchase-token body shared by the Google and Huawei routes: both
/// identify a purchase by a token string rather than by a receipt blob, so one
/// shape serves `POST /v2/iap/purchase/{google,huawei}`.
#[derive(Debug, Deserialize)]
pub struct PurchaseTokenRequest {
    #[serde(default)]
    pub purchase_token: String,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub store: Option<i16>,
}

/// `POST /v2/iap/subscription/{apple,google,huawei}`.
#[derive(Debug, Deserialize)]
pub struct SubscriptionRequest {
    #[serde(default)]
    pub receipt: String,
    #[serde(default)]
    pub purchase_token: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub store: Option<i16>,
    #[serde(default)]
    pub expire_time: Option<DateTime<Utc>>,
}

/// `api.ValidatePurchaseResponse`'s purchase, as BaaS marshals it.
#[derive(Debug, Serialize)]
pub struct PurchaseResponse {
    product_id: String,
    transaction_id: String,
    purchase_time: i64,
    update_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    refund_time: Option<i64>,
    environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_notification: Option<String>,
}

/// `api.ValidateSubscriptionResponse`'s subscription, as BaaS marshals it.
#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    product_id: String,
    transaction_id: String,
    purchase_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refund_time: Option<i64>,
    environment: String,
    original_transaction_id: String,
    state: i32,
}

/// `{"purchases": [...], "next_cursor": "..."}`.
#[derive(Debug, Serialize)]
pub struct PurchaseListResponse {
    pub purchases: Vec<PurchaseResponse>,
    pub next_cursor: String,
}

/// `{"subscriptions": [...], "next_cursor": "..."}`.
#[derive(Debug, Serialize)]
pub struct SubscriptionListResponse {
    pub subscriptions: Vec<SubscriptionResponse>,
    pub next_cursor: String,
}

fn environment_name(env: i16) -> String {
    match env {
        1 => "SANDBOX".into(),
        2 => "PROD".into(),
        _ => "UNKNOWN".into(),
    }
}

impl From<&Purchase> for PurchaseResponse {
    fn from(p: &Purchase) -> Self {
        PurchaseResponse {
            product_id: p.product_id.clone(),
            transaction_id: p.transaction_id.clone(),
            purchase_time: p.purchase_time.timestamp(),
            update_time: p.update_time.timestamp(),
            refund_time: (p.refund_time.timestamp() > 0).then(|| p.refund_time.timestamp()),
            environment: environment_name(p.environment),
            provider_notification: None,
        }
    }
}

impl From<&Subscription> for SubscriptionResponse {
    fn from(s: &Subscription) -> Self {
        SubscriptionResponse {
            product_id: s.product_id.clone(),
            transaction_id: s.original_transaction_id.clone(),
            purchase_time: s.purchase_time.timestamp(),
            expiration_time: (s.expire_time.timestamp() > 0).then(|| s.expire_time.timestamp()),
            refund_time: (s.refund_time.timestamp() > 0).then(|| s.refund_time.timestamp()),
            environment: environment_name(s.environment),
            original_transaction_id: s.original_transaction_id.clone(),
            // state: 1 active (no refund, not expired today) else 3 expired.
            state: if s.refund_time.timestamp() > 0 {
                0
            } else if s.expire_time.timestamp() > 0 && s.expire_time < Utc::now() {
                3
            } else {
                1
            },
        }
    }
}
