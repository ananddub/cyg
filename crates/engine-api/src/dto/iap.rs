use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::{Purchase, Subscription};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

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

#[derive(Debug, Deserialize)]
pub struct PurchaseTokenRequest {
    #[serde(default)]
    pub purchase_token: String,
    #[serde(default)]
    pub signed_request: Option<String>,
    #[serde(default)]
    pub purchase_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub store: Option<i16>,
}

impl PurchaseTokenRequest {
    pub(crate) fn credential(&self) -> &str {
        if !self.purchase_token.is_empty() {
            return &self.purchase_token;
        }
        self.signed_request
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| self.purchase_id.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or_default()
    }
}

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

#[derive(Debug, Serialize)]
pub struct PurchaseListResponse {
    pub purchases: Vec<PurchaseResponse>,
    pub next_cursor: String,
}

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

impl PurchaseResponse {
    pub(crate) fn parsed(product_id: &str, transaction_id: &str) -> Self {
        let now = Utc::now().timestamp();
        PurchaseResponse {
            product_id: product_id.to_owned(),
            transaction_id: transaction_id.to_owned(),
            purchase_time: now,
            update_time: now,
            refund_time: None,
            environment: environment_name(0),
            provider_notification: None,
        }
    }
}

impl SubscriptionResponse {
    pub(crate) fn parsed(product_id: &str, transaction_id: &str) -> Self {
        SubscriptionResponse {
            product_id: product_id.to_owned(),
            transaction_id: transaction_id.to_owned(),
            purchase_time: Utc::now().timestamp(),
            expiration_time: None,
            refund_time: None,
            environment: environment_name(0),
            original_transaction_id: transaction_id.to_owned(),
            state: 1,
        }
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
