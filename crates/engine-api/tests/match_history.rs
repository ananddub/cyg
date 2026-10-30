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
use engine_api::repositories::models::{DesyncReport, GameplayEvent, MatchHistory};

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

/// A finished match is only useful if it can be read back after the room is
/// gone: the summary rows, then the replay bundle that stitches the saved
/// snapshot to the event stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_finished_match_is_readable_and_replayable() {
    let env = Harness::start().await;
    let queries = Queries::new(
        env.pool
            .clone()
            .expect("the harness opens the mem:// datastore"),
    );

    let started = Utc::now() - chrono::Duration::seconds(90);
    queries
        .record_match(&MatchHistory {
            id: Uuid::new_v4(),
            game_id: "tank-3d".into(),
            room: "arena".into(),
            end_reason: "duration".into(),
            started_at: started,
            ended_at: started + chrono::Duration::seconds(90),
            ticks: 2_700,
            peak_players: 4,
            snapshot_seq: 6,
            result: vec![0xd4, 0x02],
            create_time: Utc::now(),
        })
        .await
        .expect("a finished match is stored");
    queries
        .save_room_snapshot("arena", "wasm", &[0xab, 0xcd], 3, 6)
        .await
        .expect("a host snapshot is stored");
    queries
        .record_gameplay_event(&GameplayEvent {
            id: Uuid::new_v4(),
            room: "arena".into(),
            tick: 2_699,
            seq: 1,
            player_id: 0,
            event: "match_end".into(),
            payload: vec![1],
            create_time: Utc::now(),
        })
        .await
        .expect("the closing event is stored");

    let history = json_body(
        env.client
            .get(format!("{}/v2/match/arena/history", env.base))
            .bearer_auth(player_token())
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the room's finished matches",
    )
    .await;
    assert_eq!(history.as_array().map(|v| v.len()), Some(1), "{history}");
    assert_eq!(history[0]["end_reason"], "duration");
    assert_eq!(history[0]["peak_players"], 4);
    // The outcome the guest declared, and the save the replay starts from.
    assert_eq!(history[0]["result"], "1AI=", "{history}");
    assert_eq!(history[0]["snapshot_seq"], 6);

    let console = console_login(&env.client, &env.base).await;
    let replay = json_body(
        env.client
            .get(format!("{}/v2/console/match/arena/replay", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the replay bundle",
    )
    .await;
    assert_eq!(replay["backend"], "wasm");
    // The bundle says which save it is holding, so a reader can check it
    // against the match row's `snapshot_seq` instead of assuming.
    assert_eq!(replay["snapshot_seq"], 6);
    // The snapshot travels base64, the same way every other byte column does.
    assert_eq!(replay["snapshot"], "q80=", "{replay}");
    assert_eq!(replay["matches"].as_array().map(|v| v.len()), Some(1));
    assert_eq!(replay["events"].as_array().map(|v| v.len()), Some(1));

    // A room that never persisted anything still answers with its rows: the
    // events are the answer, and a 404 would throw them away.
    let empty = json_body(
        env.client
            .get(format!("{}/v2/console/match/ghost/replay", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a room with no snapshot",
    )
    .await;
    assert!(empty["snapshot"].is_null(), "{empty}");
    assert!(empty["snapshot_seq"].is_null(), "{empty}");
    assert_eq!(empty["events"].as_array().map(|v| v.len()), Some(0));

    let denied = env
        .client
        .get(format!("{}/v2/console/match/arena/replay", env.base))
        .bearer_auth(player_token())
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        denied,
        StatusCode::UNAUTHORIZED,
        "a player token must not read a console replay"
    );

    env.stop().await;
}
