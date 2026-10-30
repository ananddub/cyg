use std::time::Duration;

use uuid::Uuid;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod buy_flow;
mod listing_escrow;
mod wager_flow;

pub use buy_flow::*;
pub use listing_escrow::*;
pub use wager_flow::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn marketplace_escrow_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    listing_escrow(&env, &console).await;
    buy_flow(&env, &console).await;
    wager_flow(&env, &console).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the marketplace and wager surface passed");
}

pub async fn define_item(env: &Harness, console: &str, item_id: &str) {
    json_body(
        env.client
            .post(format!("{}/v2/inventory/define", env.base))
            .bearer_auth(console)
            .json(&json!({
                "item_id": item_id,
                "name": item_id,
                "category": "market",
                "max_stack": 5,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "define a tradable item",
    )
    .await;
}

/// Puts one instance of a fresh item into a player's pack and returns its id.
pub async fn grant_instance(env: &Harness, uid: &str, item_id: &str) -> String {
    let granted = json_body(
        env.client
            .post(format!("{}/v2/inventory/grant", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .header("Idempotency-Key", format!("grant-{uid}-{item_id}"))
            .json(&json!({"user_id": uid, "item_id": item_id, "quantity": 1}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "grant the seller an instance",
    )
    .await;
    granted["item"]["instance_id"]
        .as_str()
        .expect("a granted instance")
        .to_owned()
}

pub async fn fund(env: &Harness, console: &str, uid: &str, gold: f64) {
    json_body(
        env.client
            .post(format!("{}/v2/console/account/{uid}/wallet", env.base))
            .bearer_auth(console)
            .header("Idempotency-Key", format!("fund-{uid}-{gold}"))
            .json(&json!({"changeset": {"gold": gold}, "metadata": {"why": "test float"}}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "put spendable gold on the account",
    )
    .await;
}

pub async fn gold_of(env: &Harness, console: &str, uid: &str) -> f64 {
    let account = json_body(
        env.client
            .get(format!("{}/v2/console/account/{uid}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the wallet",
    )
    .await;
    account["wallet"]["gold"].as_f64().unwrap_or(0.0)
}

pub async fn inventory_of(env: &Harness, token: &str) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!("{}/v2/inventory", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list the inventory",
    )
    .await;
    listed["items"].as_array().expect("an item list").clone()
}

pub async fn holds(env: &Harness, token: &str, instance: &str) -> Option<Value> {
    inventory_of(env, token)
        .await
        .into_iter()
        .find(|item| item["instance_id"] == json!(instance))
}

pub async fn list_item(
    env: &Harness,
    token: &str,
    instance: &str,
    price: f64,
) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/marketplace/listings", env.base))
        .bearer_auth(token)
        .json(&json!({
            "instance_id": instance,
            "price_currency": "gold",
            "price": price,
        }))
        .send()
        .await
        .unwrap()
}

pub async fn buy_listing(env: &Harness, token: &str, listing: &str) -> reqwest::Response {
    env.client
        .post(format!(
            "{}/v2/marketplace/listings/{listing}/buy",
            env.base
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

pub async fn cancel_listing(env: &Harness, token: &str, listing: &str) -> reqwest::Response {
    env.client
        .post(format!(
            "{}/v2/marketplace/listings/{listing}/cancel",
            env.base
        ))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

pub async fn wager(env: &Harness, body: Value) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/wager/create", env.base))
        .header(reqwest::header::AUTHORIZATION, server_key_header())
        .json(&body)
        .send()
        .await
        .unwrap()
}

pub async fn settle_wager(env: &Harness, wager_id: &str, body: Value) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/wager/{wager_id}/settle", env.base))
        .header(reqwest::header::AUTHORIZATION, server_key_header())
        .json(&body)
        .send()
        .await
        .unwrap()
}

pub async fn refund_wager(env: &Harness, wager_id: &str) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/wager/{wager_id}/refund", env.base))
        .header(reqwest::header::AUTHORIZATION, server_key_header())
        .send()
        .await
        .unwrap()
}

pub async fn listing_row(env: &Harness, listing: &str) -> Value {
    let rows = rows_matching(env, "market_listing", listing).await;
    assert_eq!(rows.len(), 1, "the listing is one row: {rows:?}");
    rows.into_iter().next().expect("the listing")
}

pub async fn wager_row(env: &Harness, wager_id: &str) -> Value {
    let rows = rows_matching(env, "wager", wager_id).await;
    assert_eq!(rows.len(), 1, "the wager is one row: {rows:?}");
    rows.into_iter().next().expect("the wager")
}

/// Ledger rows carrying `needle` in their metadata, for asserting that a
/// refused or reversed move left no money behind.
pub async fn ledger_for(env: &Harness, needle: &str) -> Vec<Value> {
    rows_matching(env, "wallet_ledger", needle).await
}

pub fn uid() -> Uuid {
    Uuid::new_v4()
}

pub async fn rows_matching(env: &Harness, table: &str, needle: &str) -> Vec<Value> {
    let rows: Vec<Value> = env
        .db()
        .query(&format!("SELECT * FROM {table}"))
        .await
        .expect("rows from table");
    rows.into_iter()
        .filter(|row| row.to_string().contains(needle))
        .collect()
}
