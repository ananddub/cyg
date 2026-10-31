use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_purchase_reads_back_by_transaction_id() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    q.insert_purchase(&InsertPurchaseParams {
        transaction_id: "tx-1".into(),
        user_id: user,
        store: 1,
        environment: 1,
        product_id: "coins".into(),
        purchase_time: Utc::now(),
        refund_time: Utc::now(),
        raw_response: json!({"receipt": "ok"}),
    })
    .await
    .expect("the purchase stores");

    let row = q.get_purchase("tx-1").await.expect("the purchase reads");
    assert_eq!(row.user_id, user);
    assert_eq!(row.product_id, "coins");
}

#[tokio::test]
async fn a_subscription_resolves_by_product() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    q.insert_subscription(&InsertSubscriptionParams {
        original_transaction_id: "sub-1".into(),
        user_id: user,
        store: 1,
        environment: 1,
        product_id: "vip".into(),
        purchase_time: Utc::now(),
        expire_time: Utc::now(),
        refund_time: Utc::now(),
        raw_response: json!({}),
    })
    .await
    .expect("the subscription stores");

    let row = q
        .subscription_by_product(user, "vip")
        .await
        .expect("the subscription reads")
        .expect("the subscription exists");
    assert_eq!(row.original_transaction_id, "sub-1");
}
