use super::*;

pub struct InsertPurchaseParams {
    pub transaction_id: String,
    pub user_id: Uuid,
    pub store: i16,
    pub environment: i16,
    pub product_id: String,
    pub purchase_time: DateTime<Utc>,
    pub refund_time: DateTime<Utc>,
    pub raw_response: serde_json::Value,
}

pub type ListPurchasesParams = ListWalletLedgerParams;

pub struct InsertSubscriptionParams {
    pub original_transaction_id: String,
    pub user_id: Uuid,
    pub store: i16,
    pub environment: i16,
    pub product_id: String,
    pub purchase_time: DateTime<Utc>,
    pub expire_time: DateTime<Utc>,
    pub refund_time: DateTime<Utc>,
    pub raw_response: serde_json::Value,
}

pub type ListSubscriptionsParams = ListWalletLedgerParams;

pub struct InsertWalletLedgerParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub changeset: serde_json::Value,
    pub metadata: serde_json::Value,
}

pub(super) fn new_purchase(p: &InsertPurchaseParams, now: DateTime<Utc>) -> Purchase {
    Purchase {
        create_time: now,
        environment: p.environment,
        product_id: p.product_id.clone(),
        purchase_time: p.purchase_time,
        raw_response: p.raw_response.clone(),
        store: p.store,
        transaction_id: p.transaction_id.clone(),
        update_time: now,
        user_id: p.user_id,
        refund_time: p.refund_time,
        game_id: None,
    }
}

pub(super) fn new_subscription(p: &InsertSubscriptionParams, now: DateTime<Utc>) -> Subscription {
    Subscription {
        original_transaction_id: p.original_transaction_id.clone(),
        create_time: now,
        update_time: now,
        environment: p.environment,
        product_id: p.product_id.clone(),
        purchase_time: p.purchase_time,
        store: p.store,
        user_id: p.user_id,
        expire_time: p.expire_time,
        raw_response: p.raw_response.clone(),
        raw_notification: serde_json::json!({}),
        refund_time: p.refund_time,
        game_id: None,
    }
}
