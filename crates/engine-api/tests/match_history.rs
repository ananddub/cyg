//! Read side of the host's event mirror: rows the control plane stored for a
//! room come back out of the two endpoints, and a caller without a credential
//! gets neither. The queue itself is the sim's test (`engine-bus`), and the
//! allowlist is the host's (`engine-server`); what only this suite proves is
//! that the tables exist and the routes answer.

use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

use engine_api::infra::jwt::mint_token_pair;
use engine_api::repositories::db::Queries;
use engine_api::repositories::models::{DesyncReport, GameplayEvent};

const EXPECTED: u64 = 0x0123_4567_89ab_cdef;
const CLIENT: u64 = 0xfedc_ba98_7654_3210;

fn player_token() -> String {
    mint_token_pair(Uuid::new_v4(), "player", HashMap::new()).0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rooms_history_and_desyncs_are_readable_by_their_own_token() {
    let env = Harness::start().await;
    let queries = Queries::new(
        env.pool
            .clone()
            .expect("the harness opens the mem:// datastore"),
    );

    queries
        .record_gameplay_event(&GameplayEvent {
            id: Uuid::new_v4(),
            room: "arena".into(),
            tick: 7,
            seq: 1,
            player_id: 3,
            event: "discard".into(),
            payload: vec![0xde, 0xad],
            create_time: Utc::now(),
        })
        .await
        .expect("a mirrored event is stored");
    queries
        .record_desync_report(&DesyncReport {
            id: Uuid::new_v4(),
            room: "arena".into(),
            player_id: 3,
            tick: 8,
            expected_checksum: format!("{EXPECTED:016x}"),
            client_checksum: format!("{CLIENT:016x}"),
            create_time: Utc::now(),
        })
        .await
        .expect("a desync is stored");

    let rows = json_body(
        env.client
            .get(format!("{}/v2/match/arena/events", env.base))
            .bearer_auth(player_token())
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the room's event history",
    )
    .await;
    assert_eq!(rows.as_array().map(|v| v.len()), Some(1), "{rows}");
    assert_eq!(rows[0]["event"], "discard");
    assert_eq!(rows[0]["tick"], 7);

    // An unlisted room answers empty: a replay tool must be able to tell "no
    // history" from "the server is broken", and 200-with-no-rows is the first.
    let empty = json_body(
        env.client
            .get(format!("{}/v2/match/silent-room/events", env.base))
            .bearer_auth(player_token())
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a room with no history",
    )
    .await;
    assert_eq!(empty.as_array().map(|v| v.len()), Some(0), "{empty}");

    let console = console_login(&env.client, &env.base).await;
    let desyncs = json_body(
        env.client
            .get(format!("{}/v2/console/desync", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the desync ledger",
    )
    .await;
    assert_eq!(desyncs.as_array().map(|v| v.len()), Some(1), "{desyncs}");
    assert_eq!(desyncs[0]["room"], "arena");
    assert_eq!(desyncs[0]["expected_checksum"], format!("{EXPECTED:016x}"));
    assert_eq!(desyncs[0]["client_checksum"], format!("{CLIENT:016x}"));

    let narrowed = json_body(
        env.client
            .get(format!("{}/v2/console/desync?room=other", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the desync ledger for a room nobody diverged in",
    )
    .await;
    assert_eq!(narrowed.as_array().map(|v| v.len()), Some(0), "{narrowed}");

    for path in ["/v2/match/arena/events", "/v2/console/desync"] {
        let status = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{path} must not serve an anonymous caller"
        );
    }

    env.stop().await;
}
