use super::*;

/// The console half of the feature: what an operator may write, what comes back,
/// and what never comes back again.
pub async fn endpoint_surface(env: &Harness, console: &str, hook: &str) {
    let list = format!("{}/v2/console/webhooks", env.base);

    let anonymous = env.client.get(&list).send().await.unwrap();
    refusal(anonymous, StatusCode::UNAUTHORIZED, "the endpoint list").await;

    let rows = endpoint_rows(env, console).await;
    assert!(rows.is_empty(), "nothing is registered yet: {rows:?}");

    for (url, why) in [
        ("http://example.com/hook", "cleartext to the whole internet"),
        ("ftp://127.0.0.1/hook", "not a web address"),
        ("not a url", "not an address at all"),
    ] {
        let refused = env
            .client
            .post(&list)
            .bearer_auth(console)
            .json(&json!({ "label": "nope", "url": url, "events": ["*"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            refusal(refused, StatusCode::BAD_REQUEST, why).await,
            "A webhook URL has to be an https address, or http on loopback.",
            "{why} is refused for the wrong reason"
        );
    }

    let unknown_event = env
        .client
        .post(&list)
        .bearer_auth(console)
        .json(&json!({
            "label": "exploded",
            "url": hook,
            "events": ["account.exploded"],
        }))
        .send()
        .await
        .unwrap();
    let message = refusal(unknown_event, StatusCode::BAD_REQUEST, "an invented event").await;
    assert!(
        message.starts_with("A webhook event must be one of"),
        "an invented event is refused for the wrong reason: {message}"
    );
    assert!(
        message.contains("quest.completed"),
        "the refusal does not name the catalog: {message}"
    );

    let short_secret = env
        .client
        .post(&list)
        .bearer_auth(console)
        .json(&json!({ "label": "weak", "url": hook, "events": ["*"], "secret": "abc" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(short_secret, StatusCode::BAD_REQUEST, "a short secret").await,
        "A webhook secret is 16-128 characters."
    );

    let no_events = env
        .client
        .post(&list)
        .bearer_auth(console)
        .json(&json!({ "label": "deaf", "url": hook, "events": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(no_events, StatusCode::BAD_REQUEST, "an endpoint that hears nothing").await,
        "A webhook endpoint subscribes to at least one event."
    );

    let created = json_body(
        env.client
            .post(&list)
            .bearer_auth(console)
            .json(&json!({
                "label": "quests",
                "url": hook,
                "events": ["quest.completed", "quest.completed"],
                "secret": SECRET,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "register an endpoint",
    )
    .await;
    let endpoint_id = created["endpoint"]["endpoint_id"]
        .as_str()
        .expect("the new endpoint's id")
        .to_owned();
    assert_eq!(
        created["secret"],
        json!(SECRET),
        "the secret is answered once, at creation"
    );
    assert_eq!(
        created["endpoint"]["events"].as_array().expect("events").len(),
        1,
        "a duplicated event is queued once"
    );

    let listed = endpoint_rows(env, console).await;
    assert_eq!(listed.len(), 1, "one endpoint is registered");
    let row = &listed[0];
    assert_eq!(row["label"], json!("quests"));
    assert_eq!(row["is_active"], json!(true));
    assert!(
        row.get("secret").is_none(),
        "the list must not hand back the signing key: {row}"
    );

    // The signing scheme is published where the receiver is written from.
    let spec = json_body(
        env.client
            .get(format!("{}/spec", list))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the signing spec",
    )
    .await;
    assert_eq!(spec["signing"]["signature_header"], json!("X-CYG-Signature"));
    assert_eq!(spec["signing"]["replay_window_seconds"], json!(300));
    assert_eq!(spec["events"].as_array().expect("events").len(), 8);

    // An edit that leaves the secret out keeps the one the row already has, so
    // pointing the same endpoint at a new URL cannot silently change its key.
    let moved = json_body(
        env.client
            .put(format!("{list}/{endpoint_id}"))
            .bearer_auth(console)
            .json(&json!({ "label": "everything", "url": hook, "events": ["*"] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "edit an endpoint",
    )
    .await;
    assert_eq!(moved["label"], json!("everything"));
    assert!(moved.get("secret").is_none(), "an edit is not a reveal");
    assert_eq!(moved["create_time"], row["create_time"], "an edit is the same endpoint");

    let dropped = json_body(
        env.client
            .delete(format!("{list}/{endpoint_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete an endpoint",
    )
    .await;
    assert_eq!(dropped, json!({}), "a delete answers nothing but its own 200");
    assert!(endpoint_rows(env, console).await.is_empty());

    let gone = env
        .client
        .put(format!("{list}/{endpoint_id}"))
        .bearer_auth(console)
        .json(&json!({ "label": "ghost", "url": hook, "events": ["*"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(gone, StatusCode::NOT_FOUND, "an endpoint that is gone").await,
        "Webhook endpoint not found."
    );
}

pub async fn create_endpoint(env: &Harness, console: &str, body: Value) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/webhooks", env.base))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "register a webhook endpoint",
    )
    .await
}

pub async fn endpoint_rows(env: &Harness, console: &str) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!("{}/v2/console/webhooks", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the endpoint list",
    )
    .await;
    listed["endpoints"]
        .as_array()
        .expect("an endpoint list")
        .clone()
}

pub async fn deliveries_of(env: &Harness, console: &str, endpoint_id: &str) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!(
                "{}/v2/console/webhooks/{endpoint_id}/deliveries",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the delivery log",
    )
    .await;
    listed["deliveries"]
        .as_array()
        .expect("a delivery list")
        .clone()
}

pub fn delivery_row(rows: &[Value], event_id: &str) -> Value {
    rows.iter()
        .find(|row| row["event_id"] == json!(event_id))
        .unwrap_or_else(|| panic!("no delivery for {event_id} in {rows:?}"))
        .clone()
}
