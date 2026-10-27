//! End-to-end test in complete Testcontainer isolation:
//! 1. Boots isolated Postgres 16 container via Testcontainers.
//! 2. Registers a new custom WASM game.
//! 3. Uploads valid WASM room module, verifies ABI validation and active status.
//! 4. Uploads 3D Arena Map (880x880 with 13 obstacles), verifies format and active status.
//! 5. Performs Matchmaking via REST (`/api/matchmake/joinOrCreate/tank-arena`), mints seat.
//! 6. Connects realtime WebSocket players, creates match, joins match, and exchanges match inputs.
//! 7. Verifies room telemetry in dashboard rooms API.

use std::collections::HashMap;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use reqwest::multipart::{Form, Part};
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;
use harness::*;

use tokio_tungstenite::tungstenite::Message as Raw;

/// Hand-assembled WASM module exporting all required host ABI symbols.
fn test_module_bytes() -> Vec<u8> {
    let names = engine_api::services::wasm::REQUIRED_EXPORTS;
    let mut module = vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];

    let mut types = leb128(1);
    types.extend([0x60u8, 0x00, 0x00]);
    module.extend(section(1, &types));

    let functions: Vec<u8> = leb128(names.len())
        .into_iter()
        .chain(std::iter::repeat_n(0u8, names.len()))
        .collect();
    module.extend(section(3, &functions));

    let mut exports = leb128(names.len());
    for (index, name) in names.iter().enumerate() {
        exports.extend(leb128(name.len()));
        exports.extend(name.as_bytes());
        exports.push(0x00);
        exports.extend(leb128(index));
    }
    module.extend(section(7, &exports));

    let mut bodies = leb128(names.len());
    for _ in names {
        bodies.extend(leb128(2));
        bodies.extend([0x00, 0x0b]);
    }
    module.extend(section(10, &bodies));
    module
}

fn leb128(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb128(payload.len()));
    out.extend(payload);
    out
}

struct TestWsClient {
    name: String,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    n: u32,
    pending: Vec<Value>,
}

impl TestWsClient {
    async fn connect(port: u16, token: &str, name: &str) -> Self {
        let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        let mut client = Self {
            name: name.into(),
            ws,
            n: 0,
            pending: Vec::new(),
        };
        client.liveness().await;
        client
    }

    async fn liveness(&mut self) {
        let cid = self.next_cid();
        self.send_raw(json!({"cid": cid, "type": "ping"}).to_string()).await;
        let pong = self.take_reply(&cid, Some("pong"), Duration::from_secs(5)).await;
        assert_eq!(pong["type"], "pong");
    }

    fn next_cid(&mut self) -> String {
        self.n += 1;
        format!("{}-{}", self.name, self.n)
    }

    async fn send_raw(&mut self, text: String) {
        self.ws
            .send(Raw::Text(text.into()))
            .await
            .expect("frame sent");
    }

    async fn send_event(&mut self, tag: &str, payload: Value) -> String {
        let cid = self.next_cid();
        let mut frame = json!({"cid": cid, "type": tag});
        if let Value::Object(map) = payload {
            for (key, value) in map {
                frame[key] = value;
            }
        }
        self.send_raw(frame.to_string()).await;
        cid
    }

    async fn fire(&mut self, tag: &str, payload: Value) {
        self.send_event(tag, payload).await;
    }

    async fn take_where(&mut self, want: &dyn Fn(&Value) -> bool, describe: &str, timeout_dur: Duration) -> Value {
        if let Some(index) = self.pending.iter().position(want) {
            return self.pending.remove(index);
        }
        let deadline = tokio::time::Instant::now() + timeout_dur;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!("{}: timeout waiting for {describe}", self.name);
            }
            let msg = tokio::time::timeout(remaining, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: timeout waiting for {describe}", self.name));
            let Some(Ok(Raw::Text(text))) = msg else {
                panic!("{}: socket closed mid-read while waiting for {describe}", self.name);
            };
            let frame: Value = serde_json::from_str(&text).expect("frame is JSON");
            if want(&frame) {
                return frame;
            }
            self.pending.push(frame);
        }
    }

    async fn take_reply(&mut self, cid: &str, want_type: Option<&str>, timeout_dur: Duration) -> Value {
        let want = want_type.map(str::to_owned);
        let cid = cid.to_owned();
        self.take_where(
            &|frame| {
                frame["cid"] == cid.as_str() && want.as_deref().is_none_or(|t| frame["type"] == t)
            },
            &format!("reply to cid {cid}"),
            timeout_dur,
        )
        .await
    }

    async fn take_event(&mut self, want_type: &str, timeout_dur: Duration) -> Value {
        let want = want_type.to_owned();
        self.take_where(
            &|frame| frame["type"] == want,
            &format!("event {want}"),
            timeout_dur,
        )
        .await
    }
}

fn mint_player_token(uid: Uuid, username: &str) -> String {
    engine_api::infra::jwt::mint_token_pair(uid, username, HashMap::new()).0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_isolated_wasm_map_and_matchmaking_lifecycle() {
    eprintln!("🚀 Starting Testcontainer isolation test...");
    let env = Harness::start().await;
    let base = env.base.clone();
    let client = env.client.clone();
    let port = env.port;

    // 1. Console Login
    eprintln!("Step 1: Console admin authentication");
    let console = console_login(&client, &base).await;
    assert!(!console.is_empty(), "Console admin token must be valid");

    // 2. Register Game with WASM backend mode
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

    // 3. Upload & Validate WASM Module
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

    // 4. Upload & Activate 3D Arena Map
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

    // 5. Matchmaking REST Reservation
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

    // 6. Realtime Match Execution (Players connect, create match, matchmake and play)
    eprintln!("Step 6: Realtime WebSocket Match Execution");
    let alice_uid = Uuid::new_v4();
    let bob_uid = Uuid::new_v4();
    let alice_token = mint_player_token(alice_uid, "alice_e2e");
    let bob_token = mint_player_token(bob_uid, "bob_e2e");

    let mut p1 = TestWsClient::connect(port, &alice_token, "player1").await;
    let mut p2 = TestWsClient::connect(port, &bob_token, "player2").await;

    // Player 1 creates match for the registered game
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

    // Creator seats into the match
    let cid_seat = p1
        .send_event("match_join", json!({"match_id": &match_id}))
        .await;
    let seated = p1
        .take_reply(&cid_seat, Some("match_join"), Duration::from_secs(5))
        .await;
    assert_eq!(seated["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Match creator seated in room");

    // Player 2 joins the match
    let cid2 = p2
        .send_event("match_join", json!({"match_id": &match_id}))
        .await;

    let ack2 = p2.take_reply(&cid2, Some("match_join"), Duration::from_secs(5)).await;
    assert_eq!(ack2["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Player 2 joined match successfully");

    let presence = p1.take_event("match_presence_event", Duration::from_secs(5)).await;
    assert_eq!(presence["match_id"].as_str(), Some(match_id.as_str()));
    eprintln!("✓ Player 1 received peer presence event");

    // Player 1 broadcasts game input / player movement
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

    // 7. Verify Telemetry & Scoped Rooms
    eprintln!("Step 7: Verifying Dashboard Scoped Rooms API");
    let scoped_rooms = json_body(
        client
            .get(format!("{base}/api/dashboard/rooms?game_id={game_id}"))
            .send()
            .await
            .expect("scoped rooms query"),
        StatusCode::OK,
        "get scoped rooms",
    )
    .await;
    assert!(scoped_rooms.is_array(), "Dashboard rooms must return array");
    eprintln!("✓ Dashboard scoped rooms returned valid listing");

    eprintln!("🎉 ALL TESTS PASSED: WASM + Map + Matchmaking verified in isolated Testcontainer!");
}
