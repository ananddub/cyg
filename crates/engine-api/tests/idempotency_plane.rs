//! The `Idempotency-Key` contract as a property of the plane, not of individual
//! handlers: a keyed write runs once, a replay returns the stored status and
//! body, and a key reused for a different body is refused.

use reqwest::header::AUTHORIZATION;
use uuid::Uuid;
#[path = "harness/mod.rs"]
mod harness_shared;

use harness_shared::*;

async fn write_record(
    env: &Harness,
    token: &str,
    board: &str,
    score: i64,
    key: Option<&str>,
) -> reqwest::Response {
    let mut request = env
        .client
        .post(format!("{}/v1/leaderboard/{board}", env.base))
        .bearer_auth(token)
        .json(&json!({ "score": score }));
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    request.send().await.expect("write a record")
}

async fn record_rows(env: &Harness, board: &str) -> Vec<Value> {
    env.db()
        .query(&format!(
            "SELECT * FROM leaderboard_record WHERE leaderboard_id = '{board}'"
        ))
        .await
        .expect("records for the board")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_keyed_write_runs_once_and_replays_the_same_reply() {
    let env = Harness::start().await;
    let board = format!("idem_board_{}", Uuid::new_v4().simple());
    seed_leaderboard(env.db(), &board, false).await;
    let player = device_player(&env.client, &env.base, "idem").await;

    let first = write_record(&env, &player.token, &board, 41, Some("score-1")).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = first.text().await.expect("first body");

    let replay = write_record(&env, &player.token, &board, 41, Some("score-1")).await;
    assert_eq!(
        replay.status(),
        StatusCode::OK,
        "a replay repeats the status"
    );
    assert_eq!(
        replay
            .headers()
            .get("idempotency-replayed")
            .and_then(|value| value.to_str().ok()),
        Some("true"),
        "the client can tell a replay from a fresh write"
    );
    assert_eq!(
        replay.text().await.expect("replay body"),
        first_body,
        "a replay returns the stored body byte for byte"
    );
    assert_eq!(
        record_rows(&env, &board).await.len(),
        1,
        "the replay wrote nothing twice"
    );

    // A second key is a second write, so the key is not a de-duplicator of the
    // route itself.
    let second = write_record(&env, &player.token, &board, 42, Some("score-2")).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        record_rows(&env, &board).await.len(),
        1,
        "one row per owner"
    );

    // No header keeps the old behaviour exactly.
    let unkeyed = write_record(&env, &player.token, &board, 43, None).await;
    assert_eq!(unkeyed.status(), StatusCode::OK);
    assert!(
        unkeyed.headers().get("idempotency-replayed").is_none(),
        "an unkeyed write is never a replay"
    );

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_key_may_not_describe_two_different_bodies() {
    let env = Harness::start().await;
    let board = format!("idem_conflict_{}", Uuid::new_v4().simple());
    seed_leaderboard(env.db(), &board, false).await;
    let player = device_player(&env.client, &env.base, "idem").await;

    let first = write_record(&env, &player.token, &board, 10, Some("shared")).await;
    assert_eq!(first.status(), StatusCode::OK);

    let conflicting = write_record(&env, &player.token, &board, 11, Some("shared")).await;
    assert_eq!(
        conflicting.status(),
        StatusCode::BAD_REQUEST,
        "the same key with a different body is a mistake, not a retry"
    );
    let body: Value = conflicting.json().await.expect("conflict body");
    assert_eq!(body["error"]["code"], json!(3), "INVALID_ARGUMENT");

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_write_does_not_freeze_its_key() {
    let env = Harness::start().await;
    let board = format!("idem_missing_{}", Uuid::new_v4().simple());
    let player = device_player(&env.client, &env.base, "idem").await;

    // The board does not exist yet, so this is a 404.
    let refused = write_record(&env, &player.token, &board, 7, Some("retry")).await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);

    // Once it exists, the same key must be allowed to really run.
    seed_leaderboard(env.db(), &board, false).await;
    let accepted = write_record(&env, &player.token, &board, 7, Some("retry")).await;
    assert_eq!(
        accepted.status(),
        StatusCode::OK,
        "a refusal is not the answer the key is forever bound to"
    );

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_server_key_write_is_keyed_by_the_request_it_names() {
    let env = Harness::start().await;
    let board = format!("idem_server_{}", Uuid::new_v4().simple());
    seed_leaderboard(env.db(), &board, false).await;
    let player = device_player(&env.client, &env.base, "idem").await;

    let grant = || {
        env.client
            .post(format!("{}/v1/inventory/grant", env.base))
            .header(AUTHORIZATION, server_key_header())
            .header("Idempotency-Key", "grant-once")
            .json(&json!({"user_id": player.uid, "item_id": "gold", "quantity": 1}))
    };

    let first = grant().send().await.expect("first grant");
    let first_status = first.status();
    let first_body = first.text().await.expect("first grant body");
    let second = grant().send().await.expect("replayed grant");
    assert_eq!(second.status(), first_status);
    assert_eq!(second.text().await.expect("replay body"), first_body);

    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_board_lifecycle_round_trips_through_typed_replies() {
    let env = Harness::start().await;
    let board = format!("idem_lifecycle_{}", Uuid::new_v4().simple());
    let player = device_player(&env.client, &env.base, "board").await;

    let defined: Value = env
        .client
        .post(format!("{}/v1/leaderboard/define", env.base))
        .bearer_auth(&player.token)
        .json(&json!({"id": board, "title": "Typed board", "sort_order": -1}))
        .send()
        .await
        .expect("define a board")
        .json()
        .await
        .expect("definition body");
    assert_eq!(defined["id"], json!(board));
    assert_eq!(defined["title"], json!("Typed board"));
    assert_eq!(defined["sort_order"], json!(-1));

    let listed: Value = env
        .client
        .get(format!("{}/v1/leaderboard", env.base))
        .bearer_auth(&player.token)
        .send()
        .await
        .expect("list boards")
        .json()
        .await
        .expect("board list body");
    assert!(
        listed["leaderboards"]
            .as_array()
            .expect("leaderboards array")
            .iter()
            .any(|row| row["id"] == json!(board)),
        "the list names the board it just created: {listed}"
    );
    assert_eq!(
        listed["total"],
        json!(listed["leaderboards"].as_array().unwrap().len())
    );

    let deleted: Value = env
        .client
        .delete(format!("{}/v1/leaderboard/{board}/board", env.base))
        .bearer_auth(&player.token)
        .send()
        .await
        .expect("delete the board")
        .json()
        .await
        .expect("delete body");
    assert_eq!(deleted["deleted"], json!(board));

    env.stop().await;
}
