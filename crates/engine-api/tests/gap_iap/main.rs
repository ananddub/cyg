use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod entitlement_surface;
mod quest_entitlement;

pub use entitlement_surface::*;
pub use quest_entitlement::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn subscription_enforcement_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    entitlement_surface(&env, &console).await;
    quest_entitlement(&env, &console).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the subscription enforcement surface passed");
}

/// The product a subscription track is gated on.
pub const VAULT: &str = "vault.pass";

pub fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub fn hours_ago(hours: i64) -> DateTime<Utc> {
    Utc::now() - ChronoDuration::hours(hours)
}

pub fn days_ahead(days: i64) -> DateTime<Utc> {
    Utc::now() + ChronoDuration::days(days)
}

/// Records a subscription the way the store relay does: the harness verifies an
/// unverified receipt, so the expiry is whatever the test asks for.
pub async fn record_subscription(
    env: &Harness,
    token: &str,
    product_id: &str,
    expire: Option<DateTime<Utc>>,
    label: &str,
) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/iap/subscription/apple", env.base))
            .bearer_auth(token)
            .json(&json!({
                "receipt": format!("e2e-iap-{label}"),
                "product_id": product_id,
                "expire_time": expire.map(stamp),
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "record a subscription",
    )
    .await
}

/// Moves a held row's clock, which is what a renewal, a lapse or a refund looks
/// like from the database's side.
pub async fn stamp_subscription(
    env: &Harness,
    uid: &str,
    expire: Option<DateTime<Utc>>,
    refund: Option<DateTime<Utc>>,
    updated: Option<DateTime<Utc>>,
) {
    let mut sets = Vec::new();
    let mut params = vec![("user", json!(uid))];
    if let Some(expire) = expire {
        sets.push("expire_time = $expire");
        params.push(("expire", json!(stamp(expire))));
    }
    if let Some(refund) = refund {
        sets.push("refund_time = $refund");
        params.push(("refund", json!(stamp(refund))));
    }
    if let Some(updated) = updated {
        sets.push("update_time = $updated");
        params.push(("updated", json!(stamp(updated))));
    }
    exec(
        env.db(),
        &format!(
            "UPDATE subscription SET {} WHERE user_id = $user",
            sets.join(", ")
        ),
        &params,
    )
    .await;
}

pub async fn entitlements(env: &Harness, token: &str) -> Value {
    json_body(
        env.client
            .get(format!("{}/v2/iap/subscription/active", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the live entitlements",
    )
    .await
}

/// One row of the entitlement reply, by product.
pub fn row_for(reply: &Value, product_id: &str) -> Value {
    reply["subscriptions"]
        .as_array()
        .expect("an array of subscription rows")
        .iter()
        .find(|row| row["product_id"] == json!(product_id))
        .unwrap_or_else(|| panic!("no row for {product_id} in {reply}"))
        .clone()
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

pub fn lists(quests: &[Value], quest_key: &str) -> bool {
    quests
        .iter()
        .any(|quest| quest["quest_key"] == json!(quest_key))
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

/// Credits counters from the authoritative server so a quest is finished before
/// it is claimed.
pub async fn ingest(env: &Harness, events: Value) {
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
    .await;
}
