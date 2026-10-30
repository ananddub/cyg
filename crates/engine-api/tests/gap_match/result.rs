use std::time::Duration;

use base64::Engine as _;
use reqwest::Response;
use uuid::Uuid;

use super::*;

/// The server-authoritative half of a match: only the host may say what happened,
/// only about the players it was seated with, and only once per claim.
pub async fn result_surface(
    env: &Harness,
    console: &str,
    host: &Player,
    away: &Player,
    outsider: &Player,
    board: &str,
) {
    subscribe(env, console).await;
    let match_id = seat_match(&[seat(host), seat(away)]);
    let route = format!("{}/v2/match/{match_id}/result", env.base);

    refusals(env, &route, host, outsider).await;

    let first = json!({
        "leaderboard": board,
        "participants": [
            { "user_id": host.uid, "score": 100, "subscore": 7,
              "stats": { "wins": 1, "kills": 3 },
              "metadata": { "mvp": true } },
            { "user_id": away.uid, "score": 40, "subscore": 2,
              "stats": { "kills": 2 } },
        ],
    });
    let posted = json_body(
        post_result(env, &route, &first, Some("result-1")).await,
        StatusCode::OK,
        "the host reports how the match ended",
    )
    .await;
    assert_eq!(posted["success"], true);
    assert_eq!(posted["leaderboard"], json!(board));
    assert_eq!(posted["recorded"][0]["score"].as_i64(), Some(100));
    assert_eq!(
        posted["recorded"][0]["counters"],
        json!({ "kills": 3, "wins": 1 })
    );
    assert_eq!(posted["recorded"][1]["score"].as_i64(), Some(40));

    assert_counter(env, host, "wins", Some(1)).await;
    assert_counter(env, host, "kills", Some(3)).await;
    assert_counter(env, away, "kills", Some(2)).await;
    assert_counter(env, away, "wins", None).await;

    let on_board = one_record(env, board, host).await;
    assert_eq!(
        (
            on_board["score"].as_i64(),
            on_board["num_score"].as_i64(),
            on_board["subscore"].as_i64(),
        ),
        (Some(100), Some(1), Some(7)),
        "the first result is the record"
    );
    assert_eq!(
        on_board["username"],
        json!(host.username),
        "the board shows the name the seat held"
    );
    assert_eq!(on_board["metadata"]["mvp"], true);
    assert_eq!(
        count(
            env.db(),
            "webhook_delivery",
            Some("event = 'match.ended'"),
            &[],
        )
        .await,
        1,
        "the fact is owed to the receiver"
    );

    let replayed = json_body(
        post_result(env, &route, &first, Some("result-1")).await,
        StatusCode::OK,
        "the same claim answers the same way",
    )
    .await;
    assert_eq!(replayed, posted, "a retry replays, it does not re-apply");
    assert_counter(env, host, "wins", Some(1)).await;
    assert_eq!(
        one_record(env, board, host).await["num_score"].as_i64(),
        Some(1),
        "a replayed result is not counted twice"
    );

    let reused = env
        .client
        .post(&route)
        .header(reqwest::header::AUTHORIZATION, server_key_header())
        .header("Idempotency-Key", "result-1")
        .json(&json!({
            "participants": [{ "user_id": host.uid, "score": 999 }],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(reused, StatusCode::BAD_REQUEST, "a key is one request").await,
        "This Idempotency-Key was already used for a different request.",
        "the key is bound to what it first claimed"
    );

    // A second, different result on the same match: a BEST board keeps the higher
    // score and counts the submission.
    let lower = json!({
        "leaderboard": board,
        "participants": [{ "user_id": host.uid, "score": 50, "stats": { "wins": 1 } }],
    });
    let second = json_body(
        post_result(env, &route, &lower, Some("result-2")).await,
        StatusCode::OK,
        "a later result posts to the same board",
    )
    .await;
    assert_eq!(
        second["recorded"][0]["score"].as_i64(),
        Some(100),
        "a board that keeps the best answers with what it holds"
    );
    assert_counter(env, host, "wins", Some(2)).await;
    assert_eq!(
        one_record(env, board, host).await["num_score"].as_i64(),
        Some(2),
        "every reported result is a score submitted"
    );

    // Counters with no board to post to still move: the board is only the part of
    // a result that is published.
    let counters_only = json!({
        "participants": [{ "user_id": away.uid, "score": 5, "stats": { "casts": 4 } }],
    });
    let unposted = json_body(
        post_result(env, &route, &counters_only, Some("result-3")).await,
        StatusCode::OK,
        "a result with no leaderboard",
    )
    .await;
    assert_eq!(unposted["leaderboard"], Value::Null);
    assert_counter(env, away, "casts", Some(4)).await;

    tokio::time::sleep(Duration::from_millis(100)).await;
}

async fn refusals(env: &Harness, route: &str, host: &Player, outsider: &Player) {
    let body = json!({ "participants": [{ "user_id": host.uid, "score": 10 }] });

    let anonymous = env.client.post(route).json(&body).send().await.unwrap();
    assert_eq!(
        refusal(
            anonymous,
            StatusCode::UNAUTHORIZED,
            "an unauthenticated result"
        )
        .await,
        "Server key required",
        "the door opens for the server key only"
    );

    let as_player = env
        .client
        .post(route)
        .bearer_auth(&host.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(as_player, StatusCode::UNAUTHORIZED, "a player's own result").await,
        "Server key invalid",
        "a session token is not the server key"
    );

    let wrong_key = env
        .client
        .post(route)
        .header(
            reqwest::header::AUTHORIZATION,
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode("not-the-key:")
            ),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    refusal(wrong_key, StatusCode::UNAUTHORIZED, "a bad server key").await;

    let unknown = format!("{}/v2/match/{}/result", env.base, Uuid::new_v4());
    assert_eq!(
        refusal(
            post_result(env, &unknown, &body, None).await,
            StatusCode::NOT_FOUND,
            "a match nobody played",
        )
        .await,
        "Match not found.",
        "an id that was never seated"
    );

    let not_a_uuid = format!("{}/v2/match/yesterday/result", env.base);
    refusal(
        post_result(env, &not_a_uuid, &body, None).await,
        StatusCode::NOT_FOUND,
        "a match id that is not an id",
    )
    .await;

    let stranger = json!({
        "participants": [{ "user_id": outsider.uid, "score": 10 }],
    });
    assert_eq!(
        refusal(
            post_result(env, route, &stranger, None).await,
            StatusCode::BAD_REQUEST,
            "a player who was not in the match",
        )
        .await,
        "A match result can only name a player who was in the match.",
        "credit goes only to who was seated"
    );
    assert_counter(env, outsider, "wins", None).await;

    let alone = seat_match(&[seat(host)]);
    let alone_route = format!("{}/v2/match/{alone}/result", env.base);
    let twice = json!({
        "participants": [
            { "user_id": host.uid, "score": 1 },
            { "user_id": host.uid, "score": 2 },
        ],
    });
    assert_eq!(
        refusal(
            post_result(env, &alone_route, &twice, None).await,
            StatusCode::BAD_REQUEST,
            "the same player named twice",
        )
        .await,
        "A match result names each player once.",
        "one name, one credit"
    );

    let empty = json!({ "participants": [] });
    refusal(
        post_result(env, route, &empty, None).await,
        StatusCode::BAD_REQUEST,
        "a result with nobody in it",
    )
    .await;

    let loud = json!({
        "participants": [{ "user_id": host.uid, "score": 1, "stats": { "Wins!": 1 } }],
    });
    assert_eq!(
        refusal(
            post_result(env, route, &loud, None).await,
            StatusCode::BAD_REQUEST,
            "a counter name that is not a counter name",
        )
        .await,
        "A counter name is 1-64 characters of lowercase letters, digits or underscore.",
        "counters are named the way events name them"
    );

    let zero = json!({
        "participants": [{ "user_id": host.uid, "score": 1, "stats": { "wins": 0 } }],
    });
    assert_eq!(
        refusal(
            post_result(env, route, &zero, None).await,
            StatusCode::BAD_REQUEST,
            "a counter that is not raised",
        )
        .await,
        "The wins increment must be between 1 and 1000000.",
        "a result may only raise a counter"
    );

    let huge = json!({
        "participants": [{ "user_id": host.uid, "score": 9_000_000_000_000_000_000_i64 }],
    });
    refusal(
        post_result(env, route, &huge, None).await,
        StatusCode::BAD_REQUEST,
        "a score off any board",
    )
    .await;

    let bad_metadata = json!({
        "participants": [{ "user_id": host.uid, "score": 1, "metadata": [] }],
    });
    assert_eq!(
        refusal(
            post_result(env, route, &bad_metadata, None).await,
            StatusCode::BAD_REQUEST,
            "metadata that is not an object",
        )
        .await,
        "Metadata value must be JSON, if provided.",
        "the shared board refusal holds here too"
    );

    let no_board = json!({
        "leaderboard": "board-never-made",
        "participants": [{ "user_id": host.uid, "score": 1 }],
    });
    assert_eq!(
        refusal(
            post_result(env, route, &no_board, None).await,
            StatusCode::NOT_FOUND,
            "a board that does not exist",
        )
        .await,
        "Leaderboard not found.",
        "the pinned refusal for a missing board"
    );

    let nameless = json!({ "participants": [{ "score": 1 }] });
    assert_eq!(
        refusal(
            post_result(env, route, &nameless, None).await,
            StatusCode::BAD_REQUEST,
            "a participant with no user id",
        )
        .await,
        "A match result needs participants, each with a user id and a score.",
        "a body that is not a result is refused as one"
    );

    assert_eq!(
        count(env.db(), "leaderboard_record", None, &[]).await,
        0,
        "nothing above published a score"
    );
    assert_eq!(
        count(env.db(), "player_stat", None, &[]).await,
        0,
        "nothing above raised a counter"
    );
}

async fn assert_counter(env: &Harness, player: &Player, stat: &str, want: Option<i64>) {
    assert_eq!(
        counter_of(env, seat(player).uid, stat).await,
        want,
        "the {stat} counter as the host left it"
    );
}

async fn one_record(env: &Harness, board: &str, player: &Player) -> Value {
    let rows = board_rows(env, board, seat(player).uid).await;
    assert_eq!(rows.len(), 1, "one record per player per window: {rows:?}");
    rows.into_iter().next().expect("the record")
}

async fn post_result(env: &Harness, route: &str, body: &Value, key: Option<&str>) -> Response {
    let mut request = env
        .client
        .post(route)
        .header(reqwest::header::AUTHORIZATION, server_key_header());
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    request.json(body).send().await.unwrap()
}

/// A receiver on loopback that nothing answers: the fact still has to be queued,
/// because queueing is what the request promises and sending is the worker's
/// problem.
async fn subscribe(env: &Harness, console: &str) {
    json_body(
        env.client
            .post(format!("{}/v2/console/webhooks", env.base))
            .bearer_auth(console)
            .json(&json!({
                "label": "match results",
                "url": "http://127.0.0.1:1/hook",
                "events": ["match.ended"],
                "secret": "e2e-match-secret",
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "subscribe to match.ended",
    )
    .await;
}
