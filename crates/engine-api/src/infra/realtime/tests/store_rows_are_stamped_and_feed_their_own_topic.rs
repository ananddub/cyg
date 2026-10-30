use super::*;

#[tokio::test]
pub(crate) async fn store_rows_are_stamped_and_feed_their_own_topic() {
    let db = memory().await;
    for table in ["users", "purchase"] {
        db.exec(&format!("DEFINE TABLE IF NOT EXISTS {table} SCHEMALESS"))
            .await
            .expect("the table exists");
    }
    let hub = RealtimeHub::new(Some(Arc::clone(&db)));
    let game = Uuid::new_v4();
    let other = Uuid::new_v4();

    let mut purchases = hub
        .subscribe(LiveTopic::Purchases, game)
        .await
        .expect("subscribed");

    let outsider = create_user(&db, other, "outsider").await;
    let buyer = create_user(&db, game, "buyer").await;
    let queries = crate::repositories::db::Queries::new(Arc::clone(&db));
    for (user_id, transaction_id) in [(outsider, "outsider-txn"), (buyer, "buyer-txn")] {
        queries
            .insert_purchase(&crate::repositories::iap::InsertPurchaseParams {
                transaction_id: transaction_id.to_string(),
                user_id,
                store: 1,
                environment: 0,
                product_id: "gems".to_string(),
                purchase_time: chrono::Utc::now(),
                refund_time: crate::db::time::epoch(),
                raw_response: serde_json::json!({}),
            })
            .await
            .expect("the purchase row is written");
    }

    let event = next(&mut purchases).await;
    assert_eq!(event.topic, "purchases");
    assert_eq!(event.action, "create");
    assert_eq!(
        event.record, "buyer-txn",
        "the other tenant's row stays out"
    );

    let params = std::collections::BTreeMap::from([(
        "transaction_id".to_string(),
        serde_json::json!("buyer-txn"),
    )]);
    let rows: Vec<serde_json::Value> = db
        .query_params(
            "SELECT game_id FROM type::record('purchase', $transaction_id) LIMIT 1",
            &params,
        )
        .await
        .expect("the purchase reads back");
    assert_eq!(
        rows[0]["game_id"].as_str(),
        Some(game.to_string().as_str()),
        "the row carries the tenant the feed filters on"
    );
}
#[tokio::test]
pub(crate) async fn the_file_backed_engine_feeds_changes_too() {
    let root = std::env::temp_dir().join(format!("cyg-live-{}", Uuid::new_v4()));
    let endpoint = format!("surrealkv://{}", root.display());
    let db = Arc::new(
        Datastore::open(
            &endpoint,
            "live",
            "live",
            None,
            None,
            KvBudget {
                memtable_mib: Some(8),
                cache_mib: Some(8),
            },
        )
        .await
        .expect("the file-backed store opens"),
    );
    db.exec("DEFINE TABLE IF NOT EXISTS users SCHEMALESS")
        .await
        .expect("the users table exists");
    let hub = RealtimeHub::new(Some(Arc::clone(&db)));
    let game = Uuid::new_v4();

    let mut sub = hub
        .subscribe(LiveTopic::Users, game)
        .await
        .expect("subscribed");
    insert_user(&db, game, "persisted").await;
    let event = next(&mut sub).await;
    assert_eq!(event.action, "create");

    drop(sub);
    drop(hub);
    drop(db);
    let _ = std::fs::remove_dir_all(root);
}
