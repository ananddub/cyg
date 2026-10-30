use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn test_isolated_wasm_map_and_matchmaking_lifecycle() {
    eprintln!("🚀 Starting isolated end-to-end test...");
    let env = Harness::start().await;
    let base = env.base.clone();
    let client = env.client.clone();
    let port = env.port;

    eprintln!("Step 1: Console admin authentication");
    let console = console_login(&client, &base).await;
    assert!(!console.is_empty(), "Console admin token must be valid");

    eprintln!("Step 2: Registering isolated custom WASM game");
    let game = json_body(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(&console)
            .json(&json!({
                "name": "Isolated Battle Arena",
                "engine_type": "custom",
                "tick_rate": 60,
                "max_clients": 16,
                "persistent_rooms": true
            }))
            .send()
            .await
            .expect("create game request"),
        StatusCode::OK,
        "create game",
    )
    .await;

    let game_id = game["id"].as_str().expect("game_id").to_owned();
    let game_key = game["game_key"].as_str().expect("game_key").to_owned();
    assert_eq!(game["engine_type"], "custom");
    assert_eq!(game["backend_mode"], "wasm");
    eprintln!("✓ Game registered: id={game_id}, key={game_key}");

    eprintln!("Step 3: Uploading & validating WASM room module");
    let wasm_bytes = test_module_bytes();
    let form = Form::new()
        .part("file", Part::bytes(wasm_bytes.clone()).file_name("room_logic.wasm"))
        .text("source_code", "// isolated test wasm room");

    let wasm_res = json_body(
        client
            .post(format!("{base}/v2/console/games/{game_id}/wasm/upload"))
            .bearer_auth(&console)
            .multipart(form)
            .send()
            .await
            .expect("wasm upload request"),
        StatusCode::OK,
        "upload wasm",
    )
    .await;

    assert_eq!(wasm_res["abi"]["valid"], true, "WASM ABI must be valid");
    assert_eq!(wasm_res["abi"]["wasm_magic_ok"], true);
    assert_eq!(wasm_res["module"]["version"], 1);
    assert_eq!(wasm_res["module"]["is_active"], true, "Uploaded module must be active");
    eprintln!("✓ WASM room module uploaded and validated: version=1 active=true");

    eprintln!("Step 4: Uploading 3D Arena Map (tank3d_arena)");
    let map_json = json!({
        "arena_width": 880.0,
        "arena_depth": 880.0,
        "obstacles": [
            { "id": 1, "name": "bunker_nw", "kind": "bunker", "x": -180.0, "y": 11.0, "z": -150.0, "w": 50.0, "h": 22.0, "d": 50.0 },
            { "id": 2, "name": "bunker_se", "kind": "bunker", "x": 180.0, "y": 11.0, "z": 150.0, "w": 50.0, "h": 22.0, "d": 50.0 },
            { "id": 3, "name": "tower_c", "kind": "tower", "x": 0.0, "y": 20.0, "z": 0.0, "w": 30.0, "h": 40.0, "d": 30.0 }
        ]
    });
    let map_bytes = serde_json::to_vec(&map_json).unwrap();
    let map_len = map_bytes.len() as i64;

    let map_form = Form::new()
        .part("file", Part::bytes(map_bytes).file_name("tank_arena.json"))
        .text("format", "map3d");

    let map_res = json_body(
        client
            .post(format!("{base}/v2/console/games/{game_id}/maps/upload"))
            .bearer_auth(&console)
            .multipart(map_form)
            .send()
            .await
            .expect("map upload request"),
        StatusCode::OK,
        "upload map",
    )
    .await;

    let map_version = map_res["map"]["version"].as_i64().expect("map version");
    assert_eq!(map_res["map"]["name"], "tank_arena.json");
    assert_eq!(map_res["map"]["format"], "map3d");
    assert_eq!(map_res["map"]["is_active"], true, "Uploaded map must be active");
    eprintln!("✓ Map uploaded: version={map_version}, format=map3d, active=true");

    let stored = row(
        env.db(),
        "SELECT * FROM game_maps WHERE game_id = $game AND version = $version LIMIT 1",
        &[("game", json!(game_id)), ("version", json!(map_version))],
    )
    .await
    .expect("the uploaded map row is readable");
    assert!(
        stored.get("bytes").is_none(),
        "the payload must not be stored in the row: {stored}"
    );
    assert_eq!(
        stored["size_bytes"].as_i64(),
        Some(map_len),
        "the row records the payload size"
    );
    let key = stored["blob_key"].as_str().expect("a blob key");
    let fetched = env
        .blobs()
        .get(key)
        .await
        .expect("the blob is readable")
        .expect("the blob exists");
    assert_eq!(
        serde_json::from_slice::<Value>(&fetched).expect("the stored map is JSON"),
        map_json,
        "the bytes round-trip through the blob store unchanged"
    );
    eprintln!("✓ Map row keeps metadata only; payload verified in the blob store");

    eprintln!("Step 5: Testing Matchmaker joinOrCreate reservation");
    let seat = json_body(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/tank-arena-alpha"))
            .json(&json!({
                "game_id": &game_id,
                "game_key": &game_key
            }))
            .send()
            .await
            .expect("matchmake request"),
        StatusCode::OK,
        "matchmake joinOrCreate",
    )
    .await;

    let room_id = seat["roomId"].as_str().expect("roomId");
    let session_id = seat["sessionId"].as_str().expect("sessionId");
    let seat_token = seat["seatToken"].as_str().expect("seatToken");
    assert_eq!(seat["name"], "tank-arena-alpha");
    assert_eq!(seat["controlPort"], port);
    assert!(!session_id.is_empty(), "sessionId must not be empty");
    assert!(!seat_token.is_empty(), "seatToken must not be empty");
    eprintln!("✓ Matchmaker reservation minted: roomId={room_id}, sessionId={session_id}");

    eprintln!("Step 6: Realtime WebSocket Match Execution");
    let alice_uid = Uuid::new_v4();
    let bob_uid = Uuid::new_v4();
    let alice_token = mint_player_token(alice_uid, "alice_e2e");
    let bob_token = mint_player_token(bob_uid, "bob_e2e");

    let mut p1 = TestWsClient::connect(port, &alice_token, "player1").await;
    let mut p2 = TestWsClient::connect(port, &bob_token, "player2").await;

    let cid1 = p1
        .send_event(
            "match_create",
            json!({
                "name": "tank-match-1",
                "open": true,
                "tick_rate": 60,
                "label": "tank-match-1",
                "game_id": &game_id
            }),
        )
        .await;

    let ack1 = p1.take_reply(&cid1, Some("match_create_ack"), Duration::from_secs(5)).await;
    let match_id = ack1["match_id"].as_str().expect("match_id").to_owned();
    eprintln!("✓ Realtime match spawned: match_id={match_id}");

    let cid_seat = p1
        .send_event("match_join", json!({"match_id": &match_id}))
        .await;
    let seated = p1
        .take_reply(&cid_seat, Some("match_join"), Duration::from_secs(5))
        .await;
    assert_eq!(seated["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Match creator seated in room");

    let cid2 = p2
        .send_event("match_join", json!({"match_id": &match_id}))
        .await;

    let ack2 = p2.take_reply(&cid2, Some("match_join"), Duration::from_secs(5)).await;
    assert_eq!(ack2["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Player 2 joined match successfully");

    let presence = p1.take_event("match_presence_event", Duration::from_secs(5)).await;
    assert_eq!(presence["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Player 1 received peer presence event");

    p1.fire(
        "match_data_send",
        json!({
            "match_id": &match_id,
            "op_code": 1,
            "data": "{\"x\": 120.5, \"y\": 11.0, \"z\": -85.0}"
        }),
    )
    .await;
    let data = p2.take_event("match_data", Duration::from_secs(5)).await;
    assert_eq!(data["op_code"], 1);
    assert_eq!(data["data"], "{\"x\": 120.5, \"y\": 11.0, \"z\": -85.0}");
    assert_eq!(data["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Player input data transmitted and received across clients");

    eprintln!("Step 7: Verifying Dashboard Scoped Rooms API");
    let scoped_rooms = json_body(
        client
            .get(format!("{base}/api/dashboard/rooms?game_id={game_id}"))
            .bearer_auth(&console)
            .send()
            .await
            .expect("scoped rooms query"),
        StatusCode::OK,
        "get scoped rooms",
    )
    .await;
    assert!(scoped_rooms.is_array(), "Dashboard rooms must return array");
    eprintln!("✓ Dashboard scoped rooms returned valid listing");

    eprintln!("🎉 ALL TESTS PASSED: WASM + Map + Matchmaking verified in isolation!");
}
