use super::*;

use std::path::Path;

use engine_api::ApiState;
use engine_api::repositories::db::Queries;
use engine_api::repositories::models::RoomRegistry;
use chrono::Utc;
use engine_bus::{Bus, RoomOwnership, RoomPhase};
use engine_config::{ControlConfig, GameConfig, SessionConfig};

fn claim(room: &str, node: &str, peers: i64) -> RoomRegistry {
    RoomRegistry {
        room_id: room.into(),
        node_id: node.into(),
        game_id: "e2e-game".into(),
        state: "active".into(),
        peers,
        tick: 1_200,
        snapshot_seq: 3,
        update_time: Utc::now(),
    }
}

async fn seat_for(env: &Harness, method: &str, room: &str) -> StatusCode {
    env.client
        .post(format!("{}/api/matchmake/{method}/{room}", env.base))
        .body("{}")
        .send()
        .await
        .expect("matchmake answers")
        .status()
}

/// The handoff: a name another node is holding is not created a second time
/// here, and every other case still mints a seat. The lease is what makes the
/// refusal safe — without it a dead pod would own the name forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_room_owned_by_another_node_is_not_created_twice() {
    let env = Harness::start().await;
    let queries = Queries::new(env.pool.clone().expect("the harness opens a datastore"));

    queries
        .claim_room(&claim("arena", "tank-3d-other-pod", 4))
        .await
        .expect("a claim is stored");
    let message = refusal(
        env.client
            .post(format!("{}/api/matchmake/joinOrCreate/arena", env.base))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "a create for a room another node runs",
    )
    .await;
    assert!(
        message.contains("tank-3d-other-pod"),
        "the refusal names where the room actually is: {message}"
    );

    // Our own claim is not a conflict: the same room takes more than one seat.
    let ours = claim("lobby", &engine_config::node_id(), 1);
    queries
        .claim_room(&ours)
        .await
        .expect("our own room is claimed");
    assert_eq!(
        seat_for(&env, "joinOrCreate", "lobby").await,
        StatusCode::OK,
        "a room this node runs keeps seating players"
    );

    // A stale row is a node that stopped beating, not a room that is live: the
    // name is free, and refusing it would strand the player with no owner at all.
    let mut ghost = claim("ghost", "tank-3d-other-pod", 0);
    ghost.update_time = Utc::now() - chrono::Duration::seconds(60);
    queries
        .claim_room(&ghost)
        .await
        .expect("a lapsed claim is stored");
    assert_eq!(
        seat_for(&env, "joinOrCreate", "ghost").await,
        StatusCode::OK,
        "a room whose owner died may be made here"
    );

    // A released name never reaches the lease question at all.
    queries
        .release_room("ghost")
        .await
        .expect("the release runs");
    assert!(
        queries
            .load_room_registry("ghost")
            .await
            .expect("the registry reads back")
            .is_none(),
        "a released room leaves no row behind"
    );
    assert_eq!(
        seat_for(&env, "joinOrCreate", "ghost").await,
        StatusCode::OK
    );

    // The seat the owner minted still works from here: every node shares the seat
    // secret, so a ticket is the attach path, and the refusal above is a "make
    // your next room somewhere else" rather than a player stranded mid-match.
    let before = json_body(
        env.client
            .post(format!("{}/api/matchmake/joinOrCreate/carry", env.base))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a room with no owner yet",
    )
    .await;
    let token = before["reconnectionToken"].as_str().expect("a seat token");
    queries
        .claim_room(&claim("carry", "tank-3d-other-pod", 2))
        .await
        .expect("the room is taken over by another node");
    assert_eq!(
        seat_for(&env, "joinOrCreate", "carry").await,
        StatusCode::CONFLICT,
        "creating it again here is refused"
    );
    assert_eq!(
        env.client
            .post(format!("{}/api/matchmake/reconnect/carry", env.base))
            .body(json!({ "reconnectionToken": token }).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the ticket the owner handed out still seats its player here"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    env.stop().await;
    eprintln!("gap: the room registry handoff passed");
}

/// The console reads the registry across nodes, which no other room surface of
/// this process can do: `/api/rooms` is one host's table.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_console_sees_which_node_runs_each_room() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;
    let queries = Queries::new(env.pool.clone().expect("the harness opens a datastore"));

    queries
        .claim_room(&claim("arena", "tank-3d-other-pod", 4))
        .await
        .expect("a claim is stored");

    let list = json_body(
        env.client
            .get(format!("{}/v2/console/room", env.base))
            .bearer_auth(&console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the console's room table",
    )
    .await;
    let rows = list["rooms"].as_array().expect("a room list");
    assert_eq!(rows.len(), 1, "{list}");
    assert_eq!(rows[0]["room_id"], "arena");
    assert_eq!(rows[0]["node_id"], "tank-3d-other-pod");
    assert_eq!(rows[0]["state"], "active");
    assert_eq!(rows[0]["peers"], 4);
    assert_eq!(rows[0]["stale"], false);

    // An unauthenticated caller gets nothing: the table names the pods.
    assert_eq!(
        env.client
            .get(format!("{}/v2/console/room", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    env.stop().await;
    eprintln!("gap: the console's room registry passed");
}

/// The pump's own control block. It reads and writes the datastore and the bus
/// and nothing else, so the surfaces that need real credentials (console login,
/// seat HMAC) are left at their defaults and no server is bound.
fn pump_control() -> ControlConfig {
    ControlConfig {
        api_port: 0,
        seat_secret: "e2e-seat-secret".into(),
        session: SessionConfig {
            database_endpoint: "mem://".into(),
            ..SessionConfig::default()
        },
        ..ControlConfig::default()
    }
}

/// The host's half of the table: it holds no database handle, so a claim becomes
/// a row only because the control plane pumps the queue, and a `Closed` row is
/// taken out rather than marked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_host_beat_becomes_a_row_and_a_release_removes_it() {
    let control = pump_control();
    let game = GameConfig {
        name: GAME_NAME.into(),
        ..GameConfig::default()
    };
    let bus = Bus::new(16, 16);
    let state = ApiState::new(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &game,
        &control,
        bus.clone(),
        0,
    )
    .await;
    // The pump and its datastore, without `serve`: the tables have to be defined
    // before a claim has somewhere to go.
    let db = state
        .db
        .clone()
        .expect("the control block opens the mem:// datastore");
    engine_api::db::schema::ensure_schema(&db)
        .await
        .expect("the schema is defined");
    let queries = Queries::new(db);
    let pump = state.clone();
    tokio::spawn(engine_api::pump_events(pump));

    let beat = |state: RoomPhase| RoomOwnership {
        room: "arena".into(),
        node: "tank-3d-7f9c".into(),
        game: "e2e-game".into(),
        state,
        peers: 2,
        tick: 900,
        snapshot_seq: 4,
    };
    bus.push_ownership(beat(RoomPhase::Active));

    let seen = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(row) = queries
                .load_room_registry("arena")
                .await
                .expect("the registry reads back")
            {
                break row;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the pump wrote the claim");
    assert_eq!(seen.node_id, "tank-3d-7f9c");
    assert_eq!(seen.state, "active");
    assert_eq!(seen.peers, 2);
    assert_eq!(seen.tick, 900);
    assert_eq!(seen.snapshot_seq, 4);

    // A second beat is an update of the same row, not a second owner of one name.
    bus.push_ownership(beat(RoomPhase::Hibernating));
    let until_hibernating = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows = queries.list_room_registry().await.expect("the list reads back");
            if rows.first().is_some_and(|row| row.state == "hibernating") {
                break rows;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the pump refreshed the claim");
    assert_eq!(until_hibernating.len(), 1, "one room, one row");

    bus.push_ownership(beat(RoomPhase::Closed));
    tokio::time::timeout(Duration::from_secs(10), async {
        while queries
            .load_room_registry("arena")
            .await
            .expect("the registry reads back")
            .is_some()
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the release took the row out");
    eprintln!("gap: the host's registry beats passed");
}
