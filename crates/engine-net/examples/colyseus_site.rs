//! Colyseus SDK demo: multi-room, WASD multi-player, room logic in WASM.
//!
//! Terminal 1 (this server):   cargo run --release -p engine-net --example colyseus_site
//! Terminal 2 (website host):  ./run_website.sh
//! Browser:                    http://localhost:8090/website.html   (one tab per player!)
//!
//! Colyseus features implemented on this Rust stack:
//!   - matchmake joinOrCreate / reconnect (seat reservations, CORS, errors)
//!   - multiple named rooms, each with its own WASM simulation instance
//!   - max-clients rule enforced inside the WASM room (is_full)
//!   - room lifecycle: created on first join, disposed 30s after going empty
//!   - reconnection: disconnecting players keep their seat + position 60s
//!   - 20 Hz schema patches via ROOM_STATE / ROOM_STATE_PATCH

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine_http::Response;
use engine_net::MatchMakeError;
use engine_net::colyseus_state::ColyseusRoomState;
use engine_net::framing::OpCode;
use engine_net::transport::{ColyseusWsTransport, GameTransport, TransportEvent};
use engine_wasm::{WasmRuntime, wasm_module};

const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 500.0;
const SPEED: f64 = 240.0; // px/sec
const MAX_CLIENTS: i32 = 16;
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

/// Parse a `ROOM_DATA_BYTES` ("input") frame delivered by the transport as
/// `OpCode::RoomDataBytes`: the protocol byte (17) is stripped, leaving
/// `[msgpack str "input"][flags u8]`.
fn parse_input_flags(payload: &[u8]) -> Option<u8> {
    let first = *payload.first()?;
    let type_len = if (0xA0..=0xBF).contains(&first) {
        (first & 0x1F) as usize
    } else {
        return None;
    };
    if payload.len() < 1 + type_len + 1 {
        return None;
    }
    Some(payload[1 + type_len])
}

/// One named room: isolated WASM simulation + schema state + members.
struct GameRoom {
    state: ColyseusRoomState,
    wasm: RoomLogic,
    /// session id -> player id (wasm-side numeric id)
    sessions: HashMap<String, i64>,
    next_player_id: i64,
    empty_since: Option<Instant>,
}

impl GameRoom {
    fn new(runtime: &WasmRuntime, wasm_module: &wasmtime::Module) -> Self {
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
        wasm.on_create(MAX_CLIENTS).expect("wasm on_create");
        Self {
            state: ColyseusRoomState::new(),
            wasm,
            sessions: HashMap::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
        }
    }
}

/// All rooms + reconnect bookkeeping for the demo server.
struct GameServer {
    runtime: WasmRuntime,
    wasm_module: wasmtime::Module,
    rooms: HashMap<String, GameRoom>,
    /// Recently disconnected players: session -> (room, x, y, when)
    recently_left: HashMap<String, (String, f64, f64, Instant)>,
}

impl GameServer {
    fn new(runtime: &WasmRuntime, wasm_bytes: &[u8]) -> Self {
        let wasm_module = runtime.compile(wasm_bytes).expect("compile room_logic");
        Self {
            runtime: runtime.clone(),
            wasm_module,
            rooms: HashMap::new(),
            recently_left: HashMap::new(),
        }
    }

    fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
        self.rooms.entry(name.to_string()).or_insert_with(|| {
            eprintln!("[server] room '{name}' created");
            GameRoom::new(&self.runtime, &self.wasm_module)
        })
    }
}

fn main() {
    // ------------------------------------------------------------------
    // 0. Website over tumhare engine-http crate se (no python needed).
    // ------------------------------------------------------------------
    let site_dir = std::env::var("COLYSEUS_DIR")
        .unwrap_or_else(|_| "/mnt/SSD/Devloper/pannel/colyseus".into());

    let read_site_file = |rel: &str| -> Vec<u8> {
        std::fs::read(format!("{site_dir}/{rel}")).unwrap_or_else(|e| {
            eprintln!("[server] missing site file {rel}: {e}");
            format!("missing {rel}").into_bytes()
        })
    };
    let sdk_js = read_site_file("packages/sdk/dist/colyseus.js");
    let schema_mjs = read_site_file("node_modules/@colyseus/schema/build/index.mjs");
    let website = read_site_file("website.html");

    let api = engine_http::Api::new()
        .get("/", {
            let website = website.clone();
            move |_| Response::new(200).with_body(website.clone())
        })
        .get("/website.html", {
            let website = website.clone();
            move |_| Response::new(200).with_body(website.clone())
        })
        .get("/packages/sdk/dist/colyseus.js", {
            let sdk_js = sdk_js.clone();
            move |_| {
                Response::new(200)
                    .with_body(sdk_js.clone())
                    .with_header("Content-Type", "text/javascript")
            }
        })
        .get(
            "/node_modules/@colyseus/schema/build/index.mjs",
            move |_| {
                Response::new(200)
                    .with_body(schema_mjs.clone())
                    .with_header("Content-Type", "text/javascript")
            },
        );
    let _http_handle = api
        .serve(
            engine_http::HttpVersion::Http1,
            "0.0.0.0:8090".parse::<SocketAddr>().unwrap(),
        )
        .expect("http serve :8090");
    eprintln!("[server] website on http://localhost:8090/website.html");

    // ------------------------------------------------------------------
    // 1. Load the WASM room logic through engine-wasm.
    // ------------------------------------------------------------------
    let runtime = WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        ..engine_wasm::WasmConfig::default()
    })
    .expect("wasm runtime");
    let wasm_bytes = include_bytes!("e2e/room_logic.wasm");
    eprintln!(
        "[server] room_logic.wasm loaded ({} bytes) — room rules + movement run in WebAssembly",
        wasm_bytes.len()
    );

    let mut transport =
        ColyseusWsTransport::new("127.0.0.1:7777".parse::<SocketAddr>().unwrap()).expect("bind");

    let server = Arc::new(Mutex::new(GameServer::new(&runtime, wasm_bytes)));

    // ------------------------------------------------------------------
    // Matchmake hook: joinOrCreate + reconnect, decided by the game.
    // ------------------------------------------------------------------
    {
        let server = server.clone();
        transport.on_matchmake(move |method, room_name, body| {
            let mut server = server.lock().unwrap();
            eprintln!("[dbg hook] locked, method={method}");

            if method == "reconnect" {
                // body: {"reconnectionToken":"<room>:<session>"}
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
                    Some((rm, _, _, at))
                        if *rm == room_name && at.elapsed() < RECONNECT_WINDOW =>
                    {
                        eprintln!("[matchmake] reconnect seat for {session}");
                        Ok(format!(
                            "{{\"name\":\"{room}\",\"sessionId\":\"{session}\",\"roomId\":\"{room}\",\"processId\":\"openoxide_1\",\"reconnectionToken\":\"{token}\"}}",
                            room = room_name,
                            session = session,
                            token = token
                        ))
                    }
                    _ => Err(MatchMakeError::expired("reconnection window expired")),
                }
            } else {
                // joinOrCreate. Capacity is enforced by the WASM is_full rule.
                if let Some(room) = server.rooms.get_mut(room_name) {
                    let count = room.wasm.get_count().unwrap_or(0);
                    let full = room.wasm.is_full(count).unwrap_or(0);
                    if full != 0 {
                        return Err(MatchMakeError::invalid_criteria(format!(
                            "room '{room_name}' is full ({count}/{MAX_CLIENTS})"
                        )));
                    }
                }
                let session = format!(
                    "colyseus_{}_{}",
                    room_name,
                    SystemTimeMillis::now()
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

    let mut last_tick = Instant::now();
    let tick_interval = Duration::from_millis(50); // 20 Hz
    let deadline = Instant::now() + Duration::from_secs(3600);

    let mut events = Vec::new();
    while Instant::now() < deadline {
        transport
            .poll_events(Duration::from_millis(5), &mut events)
            .expect("poll");

        for event in events.drain(..) {
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
                        match server.recently_left.remove(&*client_id) {
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
                    eprintln!("[dbg] locked+got room");
                    let room = server.get_or_create(&room_name);
                    eprintln!("[dbg] room ready");
                    let id = room.next_player_id;
                    room.next_player_id += 1;
                    eprintln!("[dbg] calling wasm on_join");
                    room.wasm.on_join(id, x, y).expect("wasm on_join");
                    eprintln!("[dbg] on_join ok");
                    room.state.add_player(&client_id, x, y, 0.0);
                    room.sessions.insert(client_id.to_string(), id);
                    room.empty_since = None;

                    // Full snapshot for the fresh client (non-consuming: the
                    // ADD must still broadcast to everyone else next tick).
                    // Sent as ROOM_STATE (14) per the Colyseus protocol.
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
                    // The transport drops its room mapping when it emits
                    // Disconnected; find the room via our own session tables.
                    let room_name = {
                        let server = server.lock().unwrap();
                        server
                            .rooms
                            .iter()
                            .find(|(_, r)| r.sessions.contains_key(&*client_id))
                            .map(|(name, _)| name.clone())
                            .unwrap_or_else(|| "game_room".to_string())
                    };
                    let mut server = server.lock().unwrap();
                    let (player_id, x, y) = {
                        let Some(room) = server.rooms.get_mut(&room_name) else {
                            continue;
                        };
                        let Some(&id) = room.sessions.get(&*client_id) else {
                            continue;
                        };
                        // read the player's last position from the wasm
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
                    room.sessions.remove(&*client_id);
                    room.empty_since = Some(Instant::now());
                    server
                        .recently_left
                        .insert(client_id.to_string(), (room_name.clone(), x, y, Instant::now()));
                    // Keep the transport-side reconnection token valid too.
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
                            if let Some(&id) = room.sessions.get(&*client_id) {
                                let _ = room.wasm.on_input(id, flags as i32);
                            }
                        }
                    }
                }
            }
        }

        // ---- 20 Hz: wasm tick per room + schema patch broadcast ----
        if last_tick.elapsed() >= tick_interval {
            last_tick = Instant::now();
            let dt = tick_interval.as_secs_f64();
            let mut server = server.lock().unwrap();
            let mut dead_rooms: Vec<String> = Vec::new();

            // expire stale reconnect seats
            server
                .recently_left
                .retain(|_, (_, _, _, at)| at.elapsed() < RECONNECT_WINDOW);

            for (name, room) in server.rooms.iter_mut() {
                room.wasm.update(dt, WIDTH, HEIGHT, SPEED).expect("update");

                let count = room.wasm.get_count().unwrap_or(0);
                for i in 0..count {
                    let id = room.wasm.get_id(i).unwrap_or(0);
                    let x = room.wasm.get_x(i).unwrap_or(0.0);
                    let y = room.wasm.get_y(i).unwrap_or(0.0);
                    if let Some((session, _)) = room.sessions.iter().find(|(_, pid)| **pid == id) {
                        let session = session.clone();
                        room.state.update_player(&session, x, y, 0.0);
                    }
                }
                room.state.advance_tick();

                let patch = room.state.patch();
                if !patch.is_empty() {
                    eprintln!(
                        "[dbg] patch {}B to {} sessions",
                        patch.len(),
                        room.sessions.len()
                    );
                    // broadcast to this room's members only
                    for session in room.sessions.keys() {
                        let _ = transport.send_to(session, OpCode::StateDelta, &patch);
                    }
                }

                // dispose rooms empty for > 30s
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
            transport.flush().expect("flush");
        }

        transport.flush().expect("flush");
    }
}

/// Millis since epoch (for demo seat ids).
struct SystemTimeMillis;
impl SystemTimeMillis {
    fn now() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}
