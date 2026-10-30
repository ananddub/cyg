use std::time::Instant;

use chrono::{DateTime, Duration, Utc};

use super::*;

/// The delivery half: a signed fact that lands, an endpoint that cannot be
/// reached, and the pause that stops a broken receiver from eating the queue.
pub async fn delivery_surface(env: &Harness, console: &str, taker: &Receiver, down: &Receiver) {
    let taken = create_endpoint(
        env,
        console,
        json!({
            "label": "takes everything",
            "url": taker.url,
            "events": ["*"],
            "secret": SECRET,
        }),
    )
    .await;
    let taken_id = taken["endpoint"]["endpoint_id"]
        .as_str()
        .expect("the id")
        .to_owned();
    assert_eq!(taken["secret"], json!(SECRET), "the secret comes back once");
    // A second endpoint nobody will ever trigger, which is where the generated
    // key is shown: the receiver above had to be written with the secret in hand.
    let blind = create_endpoint(
        env,
        console,
        json!({ "label": "quests only", "url": taker.url, "events": ["quest.completed"] }),
    )
    .await;
    let generated = blind["secret"]
        .as_str()
        .expect("a secret the operator did not bring")
        .to_owned();
    assert!(
        generated.starts_with("cyg_wh_") && generated.len() == 55,
        "a generated secret is not shaped like one: {generated}"
    );
    let refused = create_endpoint(
        env,
        console,
        json!({
            "label": "always says no",
            "url": down.url,
            "events": ["account.created"],
            "secret": DOWN_SECRET,
        }),
    )
    .await;
    let down_id = refused["endpoint"]["endpoint_id"]
        .as_str()
        .expect("the id")
        .to_owned();

    let first = device_player(&env.client, &env.base, "hookone").await;
    let event_id = format!("account.created:{}", first.uid);

    assert!(taker.wait_for(1).await, "the receiver never saw a delivery");
    let hit = taker.last();
    assert!(hit.valid, "the receiver refused a live delivery's signature");
    let body: Value = serde_json::from_str(&hit.body).expect("the delivery body is JSON");
    assert_eq!(body["event"], json!("account.created"));
    assert_eq!(body["event_id"], json!(event_id));
    assert_eq!(body["payload"]["user_id"], json!(first.uid));
    assert_eq!(body["payload"]["method"], json!("device"));
    assert!(
        hit.timestamp.chars().all(|digit| digit.is_ascii_digit())
            && !hit.timestamp.is_empty(),
        "the timestamp header is not wall seconds: {}",
        hit.timestamp
    );

    // The same bytes, presented after the window, are a replay.
    let late = Utc::now().timestamp() + engine_api::infra::webhook::REPLAY_WINDOW + 1;
    assert!(
        !engine_api::infra::webhook::verifies(
            SECRET,
            &hit.timestamp,
            &hit.body,
            &hit.signature,
            late
        ),
        "a captured delivery stayed valid past the window"
    );

    let rows = deliveries_of(env, console, &taken_id).await;
    let row = delivery_row(&rows, &event_id);
    assert_eq!(row["status"], json!("delivered"));
    assert_eq!(row["attempt"], json!(1));
    assert_eq!(row["response_code"], json!(200));
    assert_eq!(row["last_error"], json!(""));
    assert_eq!(
        row["delivery_id"],
        json!(hit.delivery),
        "the delivery header names the row it belongs to"
    );

    // A receiver that answers late cannot hold up the request that made the fact.
    taker.set_delay(900);
    let started = Instant::now();
    let second = device_player(&env.client, &env.base, "hooktwo").await;
    let sign_up_in = started.elapsed();
    assert!(
        sign_up_in < std::time::Duration::from_millis(400),
        "a 900ms endpoint slowed the API to {sign_up_in:?}"
    );
    assert!(taker.wait_for(2).await, "the slow delivery never landed");
    taker.set_delay(0);
    let second_body: Value =
        serde_json::from_str(&taker.last().body).expect("the second delivery is JSON");
    assert_eq!(second_body["payload"]["user_id"], json!(second.uid));

    // The endpoint that says no is retried, paused, and then let go of.
    let down_event = format!("account.created:{}", first.uid);
    let horizon = Utc::now() + Duration::seconds(1);
    let mut paused = false;
    for _ in 0..100 {
        let deferred = count(
            env.db(),
            "webhook_delivery",
            Some(
                "endpoint_id = $endpoint AND status = 'queued' AND next_retry_at > $horizon",
            ),
            &[("endpoint", json!(down_id)), ("horizon", json!(stamp(horizon)))],
        )
        .await;
        if deferred > 0 {
            paused = true;
            break;
        }
        if dead_delivery(env, console, &down_id, &down_event).await.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        paused,
        "the failing endpoint was never paused: it kept being retried immediately"
    );

    let dead = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if let Some(row) = dead_delivery(env, console, &down_id, &down_event).await {
                return row;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the exhausted delivery never reached its dead letter");
    assert_eq!(dead["attempt"], json!(MAX_ATTEMPTS));
    assert_eq!(dead["response_code"], json!(500));
    let error = dead["last_error"].as_str().expect("a reason");
    assert!(
        error.contains("answered 500"),
        "the dead letter does not say why: {error}"
    );

    // An operator can hand a dead letter back to the queue once.
    let delivery_id = dead["delivery_id"].as_str().expect("an id").to_owned();
    let again = json_body(
        env.client
            .post(format!(
                "{}/v2/console/webhooks/{down_id}/deliveries/{delivery_id}/retry",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "retry a dead letter",
    )
    .await;
    assert_eq!(again["event_id"], json!(down_event));
    let resent = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let rows = deliveries_of(env, console, &down_id).await;
            let row = delivery_row(&rows, &down_event);
            if row["attempt"].as_i64().unwrap_or(0) > MAX_ATTEMPTS {
                return row;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the retried delivery was never sent again");
    assert_eq!(
        resent["status"],
        json!("dead"),
        "a re-tried dead letter came back to life"
    );
    assert_eq!(
        resent["attempt"],
        json!(MAX_ATTEMPTS + 1),
        "the attempt count is not reset by an operator retry"
    );
}

async fn dead_delivery(env: &Harness, console: &str, endpoint_id: &str, event_id: &str) -> Option<Value> {
    let rows = deliveries_of(env, console, endpoint_id).await;
    let row = rows
        .iter()
        .find(|row| row["event_id"] == json!(event_id))
        .cloned();
    match row {
        Some(row) if row["status"] == json!("dead") => Some(row),
        _ => None,
    }
}

pub const DOWN_SECRET: &str = "e2e-webhook-down-secret-0123456789";
pub const MAX_ATTEMPTS: i64 = 4;

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}
