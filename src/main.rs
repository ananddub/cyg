//! # OpenOxide Engine — Main Server Binary
//!
//! Colyseus-SDK-compatible multi-room game server:
//!   - Room logic runs inside WebAssembly (engine-wasm + Cranelift JIT)
//!   - Matchmake (joinOrCreate / reconnect), multi-room, max-clients,
//!     reconnection seats — all protocol-compatible with @colyseus/sdk
//!   - Schema state sync (ROOM_STATE / ROOM_STATE_PATCH) at TICK_RATE Hz
//!   - Website + telemetry HTTP API served by engine-http
//!
//! Run:      cargo run --release
//! Browser:  http://localhost:8090/website.html   (WASD to move)
//!
//! Env vars:
//!   PORT=7777            game (WebSocket + matchmake) port
//!   WEB_PORT=8090        website + telemetry API port
//!   TICK_RATE=20         simulation / state broadcast Hz
//!   MAX_CLIENTS=16       per-room capacity (enforced inside the WASM room)
//!   COLYSEUS_DIR=...     directory containing website.html + SDK bundles

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine_http::{Api, Response};
use engine_net::colyseus_state::ColyseusRoomState;
use engine_net::framing::OpCode;
use engine_net::transport::{ColyseusWsTransport, GameTransport, TransportEvent};
use engine_wasm::{WasmRuntime, wasm_module};

const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 500.0;
const SPEED: f64 = 240.0; // px/sec
const RECONNECT_WINDOW: Duration = Duration::from_secs(60);

// WASD input flag bits (mirrored in website.html and room.ts).
#[allow(dead_code)]
const FLAG_W: u8 = 1;
#[allow(dead_code)]
const FLAG_S: u8 = 2;
#[allow(dead_code)]
const FLAG_A: u8 = 4;
#[allow(dead_code)]
const FLAG_D: u8 = 8;

// Type-safe wrapper over room_logic.wasm — functions are pre-cached as
// TypedFunc pointers at instantiation (~20ns per call, zero lookups).
wasm_module! {
    pub struct RoomLogic {
        fn on_create(max: i32);
        fn on_join(id: i64, x: f64, y: f64);
        fn on_leave(id: i64);
        fn on_input(id: i64, flags: i32);
        fn update(dt: f64, w: f64, h: f64, speed: f64);
        fn get_count() -> i32;
        fn get_id(i: i32) -> i64;
        fn get_x(i: i32) -> f64;
        fn get_y(i: i32) -> f64;
        fn is_full(count: i32) -> i32;
    }
}

/// Parse a `ROOM_DATA_BYTES` input frame.
/// The Custom-op arm delivers the full frame: [17][msgpack str "input"][flags u8].
fn parse_input_flags(payload: &[u8]) -> Option<u8> {
    let rest = if payload.first() == Some(&17) {
        payload.get(1..)?
    } else {
        payload
    };
    let first = *rest.first()?;
    let type_len = if (0xA0..=0xBF).contains(&first) {
        (first & 0x1F) as usize
    } else {
        return None;
    };
    if rest.len() < 1 + type_len + 1 {
        return None;
    }
    Some(rest[1 + type_len])
}

/// One named room: isolated WASM simulation + schema state + members.
struct GameRoom {
    state: ColyseusRoomState,
    wasm: RoomLogic,
    sessions: HashMap<String, i64>,
    next_player_id: i64,
    empty_since: Option<Instant>,
}

impl GameRoom {
    fn new(runtime: &WasmRuntime, wasm_module: &wasmtime::Module, max_clients: i32) -> Self {
        let mut linker = runtime.create_linker::<()>();
        linker
            .func_wrap(
                "env",
                "abort",
                |_caller: wasmtime::Caller<'_, ()>, _m: u32, _f: u32, _l: u32, _c: u32| {},
            )
            .expect("link env.abort");
        let mut wasm = RoomLogic::from_module_with_linker(runtime, &linker, wasm_module, ())
            .expect("instantiate room");
        wasm.on_create(max_clients).expect("wasm on_create");
        Self {
            state: ColyseusRoomState::new(),
            wasm,
            sessions: HashMap::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
        }
    }
}

/// All rooms + reconnect bookkeeping.
struct GameServer {
    max_clients: i32,
    runtime: WasmRuntime,
    wasm_module: wasmtime::Module,
    rooms: HashMap<String, GameRoom>,
    /// Recently disconnected players: session -> (room, x, y, when)
    recently_left: HashMap<String, (String, f64, f64, Instant)>,
}

impl GameServer {
    fn new(runtime: &WasmRuntime, wasm_bytes: &[u8], max_clients: i32) -> Self {
        let wasm_module = runtime.compile(wasm_bytes).expect("compile room_logic");
        Self {
            max_clients,
            runtime: runtime.clone(),
            wasm_module,
            rooms: HashMap::new(),
            recently_left: HashMap::new(),
        }
    }

    fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
        self.rooms.entry(name.to_string()).or_insert_with(|| {
            eprintln!("[server] room '{name}' created");
            GameRoom::new(&self.runtime, &self.wasm_module, self.max_clients)
        })
    }
}

/// Roll the whole server into a bundle the HTTP API can read without
/// contending for the game lock per field.
struct ServerStats {
    rooms: usize,
    players: usize,
    tick: u64,
}

fn main() {
    // ------------------------------------------------------------------
    // Configuration
    // ------------------------------------------------------------------
    let game_port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7777);
    let web_port: u16 = std::env::var("WEB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);
    let tick_rate: u32 = std::env::var("TICK_RATE")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(20);
    let max_clients: i32 = std::env::var("MAX_CLIENTS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(16);
    let site_dir = std::env::var("COLYSEUS_DIR")
        .unwrap_or_else(|_| "/mnt/SSD/Devloper/pannel/colyseus".into());

    println!("╔══════════════════════════════════════════════════╗");
    println!("║   OpenOxide Engine — Colyseus-compatible server  ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!("🎮 game:    ws://localhost:{game_port}");
    println!("🌐 website: http://localhost:{web_port}/website.html");
    println!("📊 telemetry: http://localhost:{web_port}/status");
    println!("⚙️  tick rate: {tick_rate} Hz | max clients/room: {max_clients}");

    // ------------------------------------------------------------------
    // 0. Website + telemetry API over the engine-http crate.
    // ------------------------------------------------------------------
    let read_site_file = |rel: &str| -> Vec<u8> {
        std::fs::read(format!("{site_dir}/{rel}")).unwrap_or_else(|e| {
            eprintln!("[server] missing site file {rel}: {e}");
            format!("missing {rel}").into_bytes()
        })
    };
    let sdk_js = read_site_file("packages/sdk/dist/colyseus.js");
    let schema_mjs = read_site_file("node_modules/@colyseus/schema/build/index.mjs");
    let website = read_site_file("website.html");

    let stats: Arc<Mutex<ServerStats>> = Arc::new(Mutex::new(ServerStats {
        rooms: 0,
        players: 0,
        tick: 0,
    }));

    let stats_api = stats.clone();
    let api = Api::new()
        .get("/health", |_| Response::text("ok"))
        .get("/", {
            let website = website.clone();
            move |_| {
                Response::new(200)
                    .with_body(website.clone())
                    .with_header("Content-Type", "text/html; charset=utf-8")
            }
        })
        .get("/website.html", {
            let website = website.clone();
            move |_| {
                Response::new(200)
                    .with_body(website.clone())
                    .with_header("Content-Type", "text/html; charset=utf-8")
            }
        })
        .get("/packages/sdk/dist/colyseus.js", {
            let sdk_js = sdk_js.clone();
            move |_| {
                Response::new(200)
                    .with_body(sdk_js.clone())
                    .with_header("Content-Type", "text/javascript")
            }
        })
        .get("/node_modules/@colyseus/schema/build/index.mjs", {
            let schema_mjs = schema_mjs.clone();
            move |_| {
                Response::new(200)
                    .with_body(schema_mjs.clone())
                    .with_header("Content-Type", "text/javascript")
            }
        })
        .get("/status", move |_| {
            let s = stats_api.lock().unwrap();
            Response::json(format!(
                r#"{{"rooms":{},"players":{},"tick":{}}}"#,
                s.rooms, s.players, s.tick
            ))
        });

    let _http_handle = Api::serve(
        api,
        engine_http::HttpVersion::Http1,
        format!("0.0.0.0:{web_port}").parse::<SocketAddr>().unwrap(),
    )
    .expect("http serve");
    eprintln!("[server] website + API on http://localhost:{web_port}");

    // ------------------------------------------------------------------
    // 1. WASM runtime — room logic compiles once, instantiates per room.
    // ------------------------------------------------------------------
    let runtime = WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        ..engine_wasm::WasmConfig::default()
    })
    .expect("wasm runtime");
    let wasm_bytes = include_bytes!("../crates/engine-net/examples/e2e/room_logic.wasm");
    eprintln!(
        "[server] room_logic.wasm loaded ({} bytes) — room rules + movement run in WebAssembly",
        wasm_bytes.len()
    );

    let mut transport = ColyseusWsTransport::new(
        format!("0.0.0.0:{game_port}")
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .expect("bind game port");

    let server = Arc::new(Mutex::new(GameServer::new(
        &runtime,
        wasm_bytes,
        max_clients,
    )));

    // ------------------------------------------------------------------
    // 2. Matchmake hook: joinOrCreate + reconnect, decided by the game.
    // ------------------------------------------------------------------
    {
        let server = server.clone();
        transport.on_matchmake(move |method, room_name, body| {
            let mut server = server.lock().unwrap();

            if method == "reconnect" {
                let token = body
                    .split("\"reconnectionToken\"")
                    .nth(1)
                    .and_then(|rest| rest.split(':').nth(1))
                    .map(|v| {
                        v.trim()
                            .trim_matches(|c| c == '"' || c == '}' || c == ' ')
                    })
                    .unwrap_or("");
                let session = token.split(':').nth(1).unwrap_or("");
                match server.recently_left.get(session) {
                    Some((rm, _, _, at)) if *rm == room_name && at.elapsed() < RECONNECT_WINDOW => {
                        eprintln!("[matchmake] reconnect seat for {session}");
                        Ok(format!(
                            "{{\"name\":\"{room}\",\"sessionId\":\"{session}\",\"roomId\":\"{room}\",\"processId\":\"openoxide_1\",\"reconnectionToken\":\"{token}\"}}",
                            room = room_name,
                            session = session,
                            token = token
                        ))
                    }
                    _ => Err(engine_net::MatchMakeError::expired(
                        "reconnection window expired",
                    )),
                }
            } else {
                // joinOrCreate. Capacity is enforced by the WASM is_full rule.
                if let Some(room) = server.rooms.get_mut(room_name) {
                    let count = room.wasm.get_count().unwrap_or(0);
                    let full = room.wasm.is_full(count).unwrap_or(0);
                    if full != 0 {
                        return Err(engine_net::MatchMakeError::invalid_criteria(format!(
                            "room '{room_name}' is full ({count}/{max_clients})"
                        )));
                    }
                }
                let session = format!(
                    "colyseus_{room_name}_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                );
                eprintln!("[matchmake] {method} '{room_name}' -> seat {session}");
                Ok(format!(
                    "{{\"name\":\"{room}\",\"sessionId\":\"{session}\",\"roomId\":\"{room}\",\"processId\":\"openoxide_1\"}}",
                    room = room_name,
                    session = session
                ))
            }
        });
    }

    // ------------------------------------------------------------------
    // 3. Game loop: 20 Hz wasm simulation + schema patch broadcast.
    // ------------------------------------------------------------------
    let mut last_tick = Instant::now();
    let tick_interval = Duration::from_millis(1000 / tick_rate.max(1) as u64);
    let mut tick: u64 = 0;

    loop {
        let events = transport
            .poll_events(Duration::from_millis(5))
            .expect("poll");

        for event in events {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    let room_name = transport
                        .room_of(&client_id)
                        .unwrap_or("game_room")
                        .to_string();

                    // Reconnection? Reuse the saved position; else spawn fresh.
                    // (Scope the first lock: the temporary guard would otherwise
                    // live until the end of the match and deadlock the arm.)
                    let (x, y) = {
                        let mut server = server.lock().unwrap();
                        match server.recently_left.remove(&client_id) {
                            Some((rm, x, y, _)) if rm == room_name => {
                                eprintln!(
                                    "[server] {client_id} RECONNECTED to '{room_name}' (seat kept)"
                                );
                                (x, y)
                            }
                            _ => {
                                let n = server
                                    .rooms
                                    .get(&room_name)
                                    .map(|r| r.sessions.len())
                                    .unwrap_or(0);
                                (
                                    60.0 + ((n as f64) * 97.0) % (WIDTH - 120.0),
                                    60.0 + ((n as f64) * 61.0) % (HEIGHT - 120.0),
                                )
                            }
                        }
                    };

                    let mut server = server.lock().unwrap();
                    let room = server.get_or_create(&room_name);
                    let id = room.next_player_id;
                    room.next_player_id += 1;
                    room.wasm.on_join(id, x, y).expect("wasm on_join");
                    room.state.add_player(&client_id, x, y);
                    room.sessions.insert(client_id.clone(), id);
                    room.empty_since = None;

                    // Full snapshot for the fresh client (non-consuming: the
                    // ADD must still broadcast to everyone else next tick).
                    let snapshot = room.state.snapshot();
                    drop(server);
                    transport
                        .send_to(&client_id, OpCode::StateSnapshot, &snapshot)
                        .expect("snapshot");
                    let _ = transport.flush();
                    eprintln!("[server] {client_id} joined room '{room_name}'");
                }
                TransportEvent::Disconnected { client_id } => {
                    eprintln!("[server] disconnected: {client_id}");
                    let room_name = transport
                        .room_of(&client_id)
                        .unwrap_or("game_room")
                        .to_string();
                    let mut server = server.lock().unwrap();
                    let (player_id, x, y) = {
                        let Some(room) = server.rooms.get_mut(&room_name) else {
                            continue;
                        };
                        let Some(&id) = room.sessions.get(&client_id) else {
                            continue;
                        };
                        let mut pos = (0.0, 0.0);
                        let count = room.wasm.get_count().unwrap_or(0);
                        for i in 0..count {
                            if room.wasm.get_id(i).unwrap_or(-1) == id {
                                pos = (
                                    room.wasm.get_x(i).unwrap_or(0.0),
                                    room.wasm.get_y(i).unwrap_or(0.0),
                                );
                                break;
                            }
                        }
                        (id, pos.0, pos.1)
                    };
                    let room = server.rooms.get_mut(&room_name).unwrap();
                    let _ = room.wasm.on_leave(player_id);
                    room.state.remove_player(&client_id);
                    room.sessions.remove(&client_id);
                    room.empty_since = Some(Instant::now());
                    server
                        .recently_left
                        .insert(client_id.clone(), (room_name.clone(), x, y, Instant::now()));
                    transport.retain_session(&room_name, &client_id);
                }
                TransportEvent::Packet {
                    client_id, payload, ..
                } => {
                    if let Some(flags) = parse_input_flags(&payload) {
                        let room_name = transport
                            .room_of(&client_id)
                            .unwrap_or("game_room")
                            .to_string();
                        let mut server = server.lock().unwrap();
                        if let Some(room) = server.rooms.get_mut(&room_name) {
                            if let Some(&id) = room.sessions.get(&client_id) {
                                let _ = room.wasm.on_input(id, flags as i32);
                            }
                        }
                    }
                }
            }
        }

        // ---- fixed-tick simulation + patch broadcast ----
        if last_tick.elapsed() >= tick_interval {
            last_tick = Instant::now();
            let dt = tick_interval.as_secs_f64();
            tick += 1;
            let mut server = server.lock().unwrap();
            let mut dead_rooms: Vec<String> = Vec::new();
            let mut total_players = 0usize;

            for (name, room) in server.rooms.iter_mut() {
                room.wasm.update(dt, WIDTH, HEIGHT, SPEED).expect("update");

                let count = room.wasm.get_count().unwrap_or(0);
                for i in 0..count {
                    let id = room.wasm.get_id(i).unwrap_or(0);
                    let x = room.wasm.get_x(i).unwrap_or(0.0);
                    let y = room.wasm.get_y(i).unwrap_or(0.0);
                    if let Some((session, _)) = room.sessions.iter().find(|(_, pid)| **pid == id) {
                        let session = session.clone();
                        room.state.update_player(&session, x, y);
                    }
                }
                room.state.advance_tick();
                total_players += room.sessions.len();

                let patch = room.state.patch();
                if !patch.is_empty() {
                    for session in room.sessions.keys() {
                        let _ = transport.send_to(session, OpCode::StateDelta, &patch);
                    }
                }

                if room.sessions.is_empty() {
                    match room.empty_since {
                        Some(t) if t.elapsed() > Duration::from_secs(30) => {
                            dead_rooms.push(name.clone());
                        }
                        Some(_) => {}
                        None => room.empty_since = Some(Instant::now()),
                    }
                }
            }

            for name in dead_rooms {
                server.rooms.remove(&name);
                eprintln!("[server] room '{name}' disposed (empty 30s)");
            }

            {
                let mut s = stats.lock().unwrap();
                s.rooms = server.rooms.len();
                s.players = total_players;
                s.tick = tick;
            }
            transport.flush().expect("flush");
        }

        transport.flush().expect("flush");
    }
}
