use super::*;

fn rule_body() -> Value {
    json!({ "all": [
        { "field": "kills", "op": "gte", "value": 10 },
        { "any": [
            { "field": "rank", "op": "eq", "value": "gold" },
            { "field": "rank", "op": "eq", "value": "platinum" }
        ]}
    ]})
}

pub async fn rules_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console rules editor surface");
    let key = format!("e2e-rule-{}", letters_suffix()).replace('_', "-");

    refusal(
        env.client
            .post(format!("{}/v2/console/rules", env.base))
            .bearer_auth(viewer)
            .json(&json!({ "key": key, "body": rule_body() }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "drafting a rule as a read-only operator",
    )
    .await;

    let message = refusal(
        env.client
            .post(format!("{}/v2/console/rules", env.base))
            .bearer_auth(console)
            .json(&json!({ "key": key, "body": { "field": "kills", "op": "between", "value": 1 } }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "drafting a rule with an unknown operator",
    )
    .await;
    assert!(
        message.starts_with("Invalid rule body at op:"),
        "the refusal names the exact field: {message}"
    );

    let draft = json_body(
        env.client
            .post(format!("{}/v2/console/rules", env.base))
            .bearer_auth(console)
            .json(&json!({ "key": key, "body": rule_body() }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "drafting a valid rule",
    )
    .await;
    assert_eq!(draft["version"], 1);
    assert_eq!(draft["is_active"], false);
    assert_eq!(draft["sha256"].as_str().map(str::len), Some(64));

    let bump = json_body(
        env.client
            .post(format!("{}/v2/console/rules", env.base))
            .bearer_auth(console)
            .json(&json!({ "key": key }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "bumping a rule clones the newest body",
    )
    .await;
    assert_eq!(bump["version"], 2);
    assert_eq!(bump["body"], rule_body());

    let published = json_body(
        env.client
            .post(format!("{}/v2/console/rules/{key}/publish/2", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "publishing version 2",
    )
    .await;
    assert_eq!(published["is_active"], true);

    let versions = json_body(
        env.client
            .get(format!("{}/v2/console/rules/{key}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "listing the rule's versions",
    )
    .await;
    assert_eq!(versions.as_array().map(Vec::len), Some(2));
    assert_eq!(versions[0]["version"], 2, "newest version first");
    assert_eq!(versions[0]["is_active"], true);
    assert_eq!(versions[1]["is_active"], false, "history is retained");

    let dry = json_body(
        env.client
            .post(format!("{}/v2/console/rules/dry-run", env.base))
            .bearer_auth(console)
            .json(&json!({ "key": key, "context": { "kills": 12, "rank": "gold" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a dry run that matches",
    )
    .await;
    assert_eq!(dry["matched"], true);

    let dry = json_body(
        env.client
            .post(format!("{}/v2/console/rules/dry-run", env.base))
            .bearer_auth(console)
            .json(&json!({ "key": key, "context": { "kills": 2, "rank": "bronze" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a dry run that misses",
    )
    .await;
    assert_eq!(dry["matched"], false);

    let deleted = json_body(
        env.client
            .delete(format!("{}/v2/console/rules/{key}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "deleting every version of the rule",
    )
    .await;
    assert_eq!(deleted["deleted"], 2);
}

pub async fn config_history_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console config history and rollback");
    let game = create_game(
        env,
        console,
        &format!("e2e-cfg-{}", letters_suffix()),
        "unity",
    )
    .await;
    let id = game.id.clone();

    json_body(
        env.client
            .put(format!("{}/v2/console/games/{id}", env.base))
            .bearer_auth(console)
            .json(&json!({ "config": { "tick_rate": 30 } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the first config update",
    )
    .await;
    json_body(
        env.client
            .put(format!("{}/v2/console/games/{id}", env.base))
            .bearer_auth(console)
            .json(&json!({ "config": { "tick_rate": 60, "max_clients": 32 } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second config update",
    )
    .await;

    let history = json_body(
        env.client
            .get(format!("{}/v2/console/games/{id}/config/history", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET config history",
    )
    .await;
    let rows = history.as_array().expect("history is an array");
    assert!(rows.len() >= 2, "every change is recorded: {history}");
    assert_eq!(rows[0]["is_active"], true, "the newest version is active");
    assert!(
        rows.windows(2)
            .all(|w| w[0]["version"].as_i64() > w[1]["version"].as_i64()),
        "history is newest first"
    );

    // v1 is the seeded default the game was created with; v2 is the first
    // explicit update this test made.
    let first_update = rows
        .iter()
        .nth(1)
        .expect("the first update recorded a version");
    let version = first_update["version"].as_i64().expect("version number");
    let single = json_body(
        env.client
            .get(format!(
                "{}/v2/console/games/{id}/config/{version}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reading one version's body",
    )
    .await;
    assert_eq!(single["body"]["tick_rate"], 30, "the first update's body");
    assert_eq!(single["sha256"], first_update["sha256"]);

    refusal(
        env.client
            .post(format!(
                "{}/v2/console/games/{id}/config/rollback/{version}",
                env.base
            ))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "rolling back as a read-only operator",
    )
    .await;

    let before_len = rows.len();
    let rolled = json_body(
        env.client
            .post(format!(
                "{}/v2/console/games/{id}/config/rollback/{version}",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "rolling back to the first version",
    )
    .await;
    assert_eq!(rolled["is_active"], true);
    assert_eq!(
        rolled["version"].as_i64(),
        Some(before_len as i64 + 1),
        "a rollback appends one new version, it never rewrites history"
    );

    let history = json_body(
        env.client
            .get(format!("{}/v2/console/games/{id}/config/history", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "history after the rollback",
    )
    .await;
    let rows = history.as_array().expect("history is an array");
    assert_eq!(rows.len(), before_len + 1);
    // The list view carries no bodies; the rollback response does.
    assert_eq!(rolled["body"]["tick_rate"], 30, "the rollback is live");

    refusal(
        env.client
            .post(format!(
                "{}/v2/console/games/{id}/config/rollback/9999",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "rolling back to a version that does not exist",
    )
    .await;
}

pub async fn timeline_and_economy_surface(env: &Harness, console: &str, viewer: &str) {
    eprintln!("gap: console player timeline and economy summary");
    let player = device_player(&env.client, &env.base, "timeline").await;

    for (amount, reason) in [(250, "e2e"), (75, "e2e-second")] {
        json_body(
            env.client
                .post(format!(
                    "{}/v2/console/account/{}/wallet",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .json(&json!({ "changeset": { "gold": amount }, "metadata": { "reason": reason } }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "credit the wallet",
        )
        .await;
    }

    let page = json_body(
        env.client
            .get(format!(
                "{}/v2/console/timeline/{}?limit=1",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the first timeline page",
    )
    .await;
    let entries = page["entries"].as_array().expect("entries");
    assert!(!entries.is_empty(), "the wallet change is on the feed");
    assert_eq!(entries[0]["kind"], "wallet");
    let first_id = entries[0]["id"].as_str().expect("entry id").to_owned();
    let cursor = page["next_cursor"].as_str().expect("a cursor for page two");

    let page = json_body(
        env.client
            .get(format!(
                "{}/v2/console/timeline/{}?limit=1&before={}",
                env.base, player.uid, cursor
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the second timeline page",
    )
    .await;
    let entries = page["entries"].as_array().expect("entries");
    assert!(
        entries.iter().all(|entry| entry["id"] != first_id.as_str()),
        "a cursor never repeats a row"
    );

    refusal(
        env.client
            .get(format!(
                "{}/v2/console/timeline/00000000-0000-0000-0000-000000000000",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "the timeline of an unknown account",
    )
    .await;
    let viewer_page = json_body(
        env.client
            .get(format!("{}/v2/console/timeline/{}", env.base, player.uid))
            .bearer_auth(viewer)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a read-only operator may read the timeline",
    )
    .await;
    assert!(
        viewer_page["entries"].as_array().map(Vec::len).unwrap_or(0) >= 1,
        "a read-only operator sees the feed"
    );

    let empty = json_body(
        env.client
            .get(format!(
                "{}/v2/console/economy/summary?from=2026-01-01&to=2026-01-02",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "an empty economy window renders zeros",
    )
    .await;
    assert_eq!(empty["empty"], true);
    assert_eq!(empty["purchases"]["total"], 0);

    let summary = json_body(
        env.client
            .get(format!("{}/v2/console/economy/summary", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the default economy window",
    )
    .await;
    assert_eq!(summary["empty"], false);
    assert!(
        summary["wallet"]["net_by_currency"]["gold"]
            .as_f64()
            .unwrap_or(0.0)
            >= 250.0,
        "the credit lands in the net: {summary}"
    );

    refusal(
        env.client
            .get(format!(
                "{}/v2/console/economy/summary?from=not-a-date",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a malformed window bound",
    )
    .await;
    refusal(
        env.client
            .get(format!(
                "{}/v2/console/economy/summary?from=2026-05-01&to=2026-01-01",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a window that ends before it starts",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn p4_devx_surface_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    let viewer_name = format!("e2e-p4view-{}", letters_suffix());
    let viewer_password = "e2e-console-viewer";
    json_body(
        env.client
            .post(format!("{}/v2/console/user", env.base))
            .bearer_auth(&console)
            .json(&json!({
                "username": viewer_name,
                "email": format!("{viewer_name}@example.com"),
                "password": viewer_password,
                "role": "readonly",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a read-only operator",
    )
    .await;
    let viewer = console_login_as(&env, &viewer_name, viewer_password, json!({})).await;

    rules_surface(&env, &console, &viewer).await;
    config_history_surface(&env, &console, &viewer).await;
    timeline_and_economy_surface(&env, &console, &viewer).await;
}
