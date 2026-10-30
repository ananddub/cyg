use std::time::Duration;

use uuid::Uuid;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod daily_claim;
mod event_ingest;
mod quest_lifecycle;
mod season_gating;

pub use daily_claim::*;
pub use event_ingest::*;
pub use quest_lifecycle::*;
pub use season_gating::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn quests_seasons_and_daily_rewards_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    quest_lifecycle(&env, &console).await;
    season_gating(&env, &console).await;
    event_ingest(&env, &console).await;
    daily_claim(&env, &console).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the quests, seasons and daily reward surface passed");
}

/// The console accepts an item definition, which is what a quest reward names.
pub async fn define_item(env: &Harness, console: &str, item_id: &str, max_stack: i64) {
    json_body(
        env.client
            .post(format!("{}/v2/inventory/define", env.base))
            .bearer_auth(console)
            .json(&json!({
                "item_id": item_id,
                "name": item_id,
                "category": "reward",
                "max_stack": max_stack,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "define a reward item",
    )
    .await;
}

pub async fn create_quest(env: &Harness, console: &str, body: Value) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/quests", env.base))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a quest",
    )
    .await
}

pub async fn create_season(env: &Harness, console: &str, body: Value) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/seasons", env.base))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a season",
    )
    .await
}

pub async fn write_season(env: &Harness, console: &str, key: &str, body: Value) -> Value {
    json_body(
        env.client
            .put(format!("{}/v2/console/seasons/{key}", env.base))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "write a season",
    )
    .await
}

/// Credits counters from the authoritative server.
pub async fn ingest(env: &Harness, events: Value) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/events", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "events": events }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "ingest server events",
    )
    .await
}

pub async fn quests_of(env: &Harness, token: &str) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!("{}/v2/quest", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list the player's quests",
    )
    .await;
    listed["quests"].as_array().expect("a quest list").clone()
}

pub async fn quest_row(env: &Harness, token: &str, quest_key: &str) -> Value {
    quests_of(env, token)
        .await
        .into_iter()
        .find(|quest| quest["quest_key"] == json!(quest_key))
        .unwrap_or_else(|| panic!("quest '{quest_key}' is not listed"))
}

pub async fn claim_quest(env: &Harness, token: &str, quest_key: &str) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/quest/claim", env.base))
        .bearer_auth(token)
        .json(&json!({ "quest_key": quest_key }))
        .send()
        .await
        .unwrap()
}

pub async fn claim_daily(env: &Harness, token: &str) -> reqwest::Response {
    env.client
        .post(format!("{}/v2/account/daily/claim", env.base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
}

pub async fn wallet_of(env: &Harness, console: &str, uid: &str) -> Value {
    let account = json_body(
        env.client
            .get(format!("{}/v2/console/account/{uid}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the account",
    )
    .await;
    account["wallet"].clone()
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

pub fn uid() -> Uuid {
    Uuid::new_v4()
}

/// Rows of one table whose JSON mentions `needle`, for asserting that a replay
/// wrote nothing.
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
