//! # ox-game server — Generic Byte Pipeline
//!
//! PS5 vs Game CD architecture: the Rust engine knows NOTHING about the
//! game. The WASM room owns all state and its own wire encoding. The host
//! is a dumb-but-fast pipe:
//!
//!   browser input  →  wasm.on_input(id, ptr, len)   (raw bytes pass-through)
//!   20-128 Hz      →  wasm.tick(dt) -> packed(ptr, len)
//!                     host reads delta bytes from wasm memory
//!                  →  broadcast [ROOM_DATA_BYTES]["delta" + bytes] to clients
//!
//! Hot-reload (Level 1): when `room_logic.wasm` changes on disk, new rooms
//! are served by the new module. Existing rooms finish on the old one —
//! no client disconnects.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine_http::{Api, Response};
use engine_net::framing::OpCode;
use engine_net::transport::{ColyseusWsTransport, GameTransport, TransportEvent};
use engine_wasm::{WasmRuntime, wasm_module};

// Type-safe wrapper over the compiled room WASM. The host never opens the
// game's bytes — it only passes pointers and reads back the delta buffer.
wasm_module! {
    pub struct RoomLogic {
        fn on_create(max: i32);
        fn on_join(id: i64, x: f64, y: f64);
        fn on_leave(id: i64);
        fn alloc(len: i32) -> i32;
        fn on_input(id: i64, ptr: i32, len: i32);
        fn tick(dt: f64) -> i64;
        fn save() -> i64;
        fn load(ptr: i32, len: i32);
    }
}

/// One named room: an isolated WASM simulation + its members.
struct GameRoom {
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
            wasm,
            sessions: HashMap::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
        }
    }
}

/// All rooms + the currently loaded wasm module (hot-swappable).
struct GameServer {
    runtime: WasmRuntime,
    wasm_module: wasmtime::Module,
    max_clients: i32,
    rooms: HashMap<String, GameRoom>,
}

impl GameServer {
    fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
        self.rooms.entry(name.to_string()).or_insert_with(|| {
            eprintln!("[server] room '{name}' created");
            GameRoom::new(&self.runtime, &self.wasm_module, self.max_clients)
        })
    }
}

/// Wrap a raw delta buffer into a Colyseus ROOM_DATA_BYTES frame payload:
/// [msgpack fixstr "delta"][raw bytes...]. The SDK surfaces it as
/// `room.onMessage("delta", bytes)`.
/// Wrap a raw delta buffer into a Colyseus ROOM_DATA_BYTES frame payload:
/// [msgpack fixstr "delta"][i64 recipient_player_id][raw bytes...].
fn wrap_delta(your_id: i64, delta: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(delta.len() + 14);
    out.push(0xA5);
    out.extend_from_slice(b"delta");
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(delta);
    out
}

/// Strip the msgpack type header from an inbound ROOM_DATA_BYTES payload:
/// payload = [msgpack str][data...] -> returns the data part.
fn strip_type_header(payload: &[u8]) -> Option<&[u8]> {
    let first = *payload.first()?;
    if (0xA0..=0xBF).contains(&first) {
        let type_len = (first & 0x1F) as usize;
        payload.get(1 + type_len..)
    } else if first == 0xD9 {
        let type_len = *payload.get(1)? as usize;
        payload.get(2 + type_len..)
    } else {
        None
    }
}

/// Run the server until killed.
pub fn run(
    project_dir: &Path,
    wasm_path: &Path,
    cfg: &crate::config::GameConfig,
) -> std::io::Result<()> {
    // ------------------------------------------------------------------
    // Website over engine-http.
    // ------------------------------------------------------------------
    let website_path = project_dir.join(&cfg.website);
    let website = if website_path.exists() {
        std::fs::read(&website_path)?
    } else {
        include_bytes!("../crates/engine-net/examples/e2e/website.html").to_vec()
    };
    let sdk_js = std::fs::read(cfg.sdk_dir.join("packages/sdk/dist/colyseus.js"))
        .expect("sdk bundle (packages/sdk/dist/colyseus.js) not found — check sdk_dir");
    let schema_mjs = std::fs::read(
        cfg.sdk_dir
            .join("node_modules/@colyseus/schema/build/index.mjs"),
    )
    .expect(
        "schema bundle (node_modules/@colyseus/schema/build/index.mjs) not found — check sdk_dir",
    );

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
        });

    let _http_handle = Api::serve(
        api,
        engine_http::HttpVersion::Http1,
        format!("0.0.0.0:{}", cfg.web_port)
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .expect("http serve");
    eprintln!(
        "[server] website on http://localhost:{}/{}",
        cfg.web_port,
        cfg.website.display()
    );

    // ------------------------------------------------------------------
    // WASM runtime — the room logic module (hot-swappable).
    // ------------------------------------------------------------------
    let runtime = WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        ..engine_wasm::WasmConfig::default()
    })
    .expect("wasm runtime");
    let wasm_bytes = std::fs::read(wasm_path)?;
    eprintln!(
        "[server] room logic loaded ({} bytes) — generic byte pipeline active",
        wasm_bytes.len()
    );

    let mut transport = ColyseusWsTransport::new(
        format!("0.0.0.0:{}", cfg.port)
            .parse::<SocketAddr>()
            .unwrap(),
    )
    .expect("bind game port");
    eprintln!("[server] game on ws://localhost:{}", cfg.port);

    let wasm_module = runtime.compile(&wasm_bytes).expect("compile room");
    let server = Arc::new(Mutex::new(GameServer {
        runtime: runtime.clone(),
        wasm_module,
        max_clients: cfg.max_clients,
        rooms: HashMap::new(),
    }));

    let mut last_tick = Instant::now();
    let tick_interval = Duration::from_millis(1000 / cfg.tick_rate.max(1) as u64);
    let mut last_wasm_check = Instant::now();
    let mut loaded_wasm_time: Option<std::time::SystemTime> =
        std::fs::metadata(wasm_path).ok().and_then(|m| m.modified().ok());

    loop {
        let events = transport
            .poll_events(Duration::from_millis(5))
            .expect("poll");

        for event in events {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    let room_name = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();

                    let (x, y) = {
                        let s = server.lock().unwrap();
                        let n = s
                            .rooms
                            .get(&room_name)
                            .map(|r| r.sessions.len())
                            .unwrap_or(0);
                        (
                            60.0 + ((n as f64) * 97.0) % (cfg.width - 120.0),
                            60.0 + ((n as f64) * 61.0) % (cfg.height - 120.0),
                        )
                    };

                    let mut server = server.lock().unwrap();
                    let room = server.get_or_create(&room_name);
                    let id = room.next_player_id;
                    room.next_player_id += 1;
                    room.wasm.on_join(id, x, y).expect("wasm on_join");
                    room.sessions.insert(client_id.clone(), id);
                    room.empty_since = None;
                    let _ = transport.flush();
                    eprintln!("[server] {client_id} joined room '{room_name}' (player id {id})");
                }
                TransportEvent::Disconnected { client_id } => {
                    eprintln!("[server] disconnected: {client_id}");
                    let room_name = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();
                    let mut server = server.lock().unwrap();
                    let Some(room) = server.rooms.get_mut(&room_name) else {
                        continue;
                    };
                    let Some(&id) = room.sessions.get(&client_id) else {
                        continue;
                    };
                    let _ = room.wasm.on_leave(id);
                    room.sessions.remove(&client_id);
                    room.empty_since = Some(Instant::now());
                }
                TransportEvent::Packet {
                    client_id, payload, ..
                } => {
                    // Raw bytes pass-through: strip the msgpack type header,
                    // copy into wasm memory, hand over the pointer.
                    let Some(data) = strip_type_header(&payload) else {
                        continue;
                    };
                    let room_name = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();
                    let mut server = server.lock().unwrap();
                    let Some(room) = server.rooms.get_mut(&room_name) else {
                        continue;
                    };
                    let Some(&id) = room.sessions.get(&client_id) else {
                        continue;
                    };

                    let ptr =
                        room.wasm.alloc(data.len() as i32).expect("wasm alloc") as u32 as usize;
                    room.wasm
                        .write_memory(ptr as u32, data)
                        .expect("write input");
                    room.wasm
                        .on_input(id, ptr as i32, data.len() as i32)
                        .expect("wasm on_input");
                }
            }
        }

        // ---- fixed-tick: wasm simulates, host broadcasts the delta buffer ----
        if last_tick.elapsed() >= tick_interval {
            last_tick = Instant::now();
            let dt = tick_interval.as_secs_f64();
            let mut server = server.lock().unwrap();
            let mut dead_rooms: Vec<String> = Vec::new();

            for (name, room) in server.rooms.iter_mut() {
                let packed = room.wasm.tick(dt).expect("wasm tick");
                let ptr = ((packed >> 32) as u32) as usize;
                let len = (packed as u32) as usize;

                if len > 0 {
                    let delta = room
                        .wasm
                        .read_memory(ptr as u32, len as u32)
                        .expect("read delta");
                    for (session, id) in &room.sessions {
                        let frame = wrap_delta(*id, &delta);
                        let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
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
            transport.flush().expect("flush");
        }

        // ---- hot-reload check (every 2s): swap module for NEW rooms ----
        if last_wasm_check.elapsed() >= Duration::from_secs(2) {
            last_wasm_check = Instant::now();
            if let Ok(current) = std::fs::metadata(wasm_path).and_then(|m| m.modified()) {
                if loaded_wasm_time != Some(current) {
                    let bytes = std::fs::read(wasm_path)?;
                    let module = runtime.compile(&bytes).expect("hot-reload compile");
                    server.lock().unwrap().wasm_module = module;
                    loaded_wasm_time = Some(current);
                    eprintln!(
                        "[server] 🔥 room logic hot-reloaded ({} bytes) — new rooms use the new module",
                        bytes.len()
                    );
                }
            }
        }

        transport.flush().expect("flush");
    }
}
