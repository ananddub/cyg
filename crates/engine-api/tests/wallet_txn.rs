use std::collections::BTreeMap;
use std::sync::Arc;

use engine_api::db::{Datastore, KvBudget, schema};
use engine_api::repositories::db::Queries;

async fn db() -> (Arc<Datastore>, Queries) {
    let db = Datastore::open("mem://", "wallet", "wallet", None, None, KvBudget::default())
        .await
        .expect("the embedded store opens");
    schema::ensure_schema(&db).await.expect("the base DDL applies");
    let db = Arc::new(db);
    let queries = Queries::new(db.clone());
    (db, queries)
}

async fn mk_user(db: &Datastore, wallet: serde_json::Value) -> uuid::Uuid {
    let user = uuid::Uuid::new_v4();
    db.exec_params(
        "CREATE type::record('users', <uuid>$id) SET username = 'wallet-txn', lang_tag = 'en', \
         metadata = {}, wallet = $wallet, edge_count = 0, password = NULL, \
         create_time = time::now(), update_time = time::now(), verify_time = time::now(), \
         disable_time = time::now();",
        &BTreeMap::from([
            ("id".to_string(), serde_json::json!(user.to_string())),
            ("wallet".to_string(), wallet),
        ]),
    )
    .await
    .expect("create user");
    user
}

fn changes(items: [(&str, f64); 1]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (currency, delta) in items {
        map.insert(currency.to_string(), serde_json::json!(delta));
    }
    map
}

async fn ledger_rows(db: &Datastore, user: uuid::Uuid) -> Vec<serde_json::Value> {
    db.query_params(
        "SELECT * FROM wallet_ledger WHERE user_id = $user;",
        &BTreeMap::from([("user".to_string(), serde_json::json!(user.to_string()))]),
    )
    .await
    .expect("list the ledger")
}

#[tokio::test]
async fn apply_wallet_change_is_atomic_and_guarded() {
    let (db, queries) = db().await;
    let user = mk_user(&db, serde_json::json!({})).await;

    let updated = queries
        .apply_wallet_change(user, &changes([("gold", 40.0)]), serde_json::json!({}), uuid::Uuid::new_v4())
        .await
        .expect("credit the wallet")
        .expect("the account exists");
    assert_eq!(updated.wallet["gold"].as_f64(), Some(40.0));

    let refused = queries
        .apply_wallet_change(user, &changes([("gold", -100.0)]), serde_json::json!({}), uuid::Uuid::new_v4())
        .await
        .expect("attempt an overdraft")
        .is_none();
    assert!(refused, "a negative balance is refused");

    let absent = queries
        .apply_wallet_change(uuid::Uuid::new_v4(), &changes([("gold", 1.0)]), serde_json::json!({}), uuid::Uuid::new_v4())
        .await
        .expect("attempt a missing account");
    assert!(absent.is_none(), "an unknown account writes nothing");

    let rows = ledger_rows(&db, user).await;
    assert_eq!(rows.len(), 1, "only the applied change reached the ledger");

    let wallet: Vec<serde_json::Value> = db
        .query_params(
            "SELECT VALUE wallet FROM type::record('users', <uuid>$id);",
            &BTreeMap::from([("id".to_string(), serde_json::json!(user.to_string()))]),
        )
        .await
        .expect("read the wallet back");
    assert_eq!(wallet.first().and_then(|w| w["gold"].as_f64()), Some(40.0));
}
