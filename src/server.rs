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


use engine_net::framing::OpCode;
use engine_net::transport::{ColyseusWsTransport, GameTransport, TransportEvent};
use engine_wasm::{WasmRuntime, wasm_module};
use crate::physics::PhysicsWorld3D;

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
    wasm: RoomLogic<Arc<PhysicsWorld3D>>,
    sessions: HashMap<String, i64>,
    spectators: std::collections::HashSet<String>,
    next_player_id: i64,
    empty_since: Option<Instant>,
}

impl GameRoom {
    fn new(
        runtime: &WasmRuntime,
        wasm_module: &wasmtime::Module,
        max_clients: i32,
        physics: Arc<PhysicsWorld3D>,
    ) -> Self {
        let mut linker = runtime.create_linker::<Arc<PhysicsWorld3D>>();
        linker
            .func_wrap(
                "env",
                "abort",
                |_caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>, _m: u32, _f: u32, _l: u32, _c: u32| {},
            )
            .expect("link env.abort");

        // Host Physics ABI 1: Surface height query (for solid standing & rooftops)
        linker
            .func_wrap(
                "env",
                "sys_physics_surface_y",
                |caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>, px: f64, py: f64, pz: f64, radius: f64| -> f64 {
                    caller.data().get_surface_height(px, py, pz, radius)
                },
            )
            .expect("link env.sys_physics_surface_y");

        // Host Physics ABI 2: Kinematic move_and_slide (smooth wall sliding along obstacle normals)
        linker
            .func_wrap(
                "env",
                "sys_physics_move",
                |mut caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>,
                 px: f64, py: f64, pz: f64,
                 vx: f64, vy: f64, vz: f64,
                 radius: f64, dt: f64,
                 out_ptr: i32| -> i32 {
                    let world = caller.data().clone();
                    let res = world.move_and_slide(px, py, pz, vx, vy, vz, radius, dt);

                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 24 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&res.new_x.to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&res.new_y.to_le_bytes());
                            data[offset + 16..offset + 24].copy_from_slice(&res.new_z.to_le_bytes());
                        }
                    }

                    if res.is_grounded { 1 } else { 0 }
                },
            )
            .expect("link env.sys_physics_move");

        // Host Physics ABI 3: Continuous Collision Detection Raycasting (bullets & hitscan)
        linker
            .func_wrap(
                "env",
                "sys_physics_raycast",
                |mut caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>,
                 ox: f64, oy: f64, oz: f64,
                 dx: f64, dy: f64, dz: f64,
                 max_dist: f64,
                 out_ptr: i32| -> i32 {
                    let world = caller.data().clone();
                    let origin = crate::physics::Vec3::new(ox, oy, oz);
                    let dir = crate::physics::Vec3::new(dx, dy, dz);

                    if let Some(hit) = world.raycast(origin, dir, max_dist) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 36 <= data.len() {
                                data[offset..offset + 8].copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16].copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24].copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32].copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36].copy_from_slice(&hit.collider_id.to_le_bytes());
                            }
                        }
                        1
                    } else {
                        0
                    }
                },
            )
            .expect("link env.sys_physics_raycast");

        // Host Physics ABI 4: Lag Compensation Entity Hitbox Recording
        linker
            .func_wrap(
                "env",
                "sys_physics_lag_record",
                |caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>,
                 tick: i64,
                 entity_id: i32,
                 kind: i32,
                 min_x: f64, min_y: f64, min_z: f64,
                 max_x: f64, max_y: f64, max_z: f64| {
                    let world = caller.data().clone();
                    let bounds = crate::physics::Aabb {
                        min: crate::physics::Vec3::new(min_x, min_y, min_z),
                        max: crate::physics::Vec3::new(max_x, max_y, max_z),
                    };
                    world.record_entity_hitbox(
                        tick as u64,
                        0,
                        entity_id as u32,
                        kind as u32,
                        bounds,
                    );
                },
            )
            .expect("link env.sys_physics_lag_record");

        // Host Physics ABI 5: Lag-Compensated Rewind Raycast (reference Fusion Server Rewind)
        linker
            .func_wrap(
                "env",
                "sys_physics_lag_raycast",
                |mut caller: wasmtime::Caller<'_, Arc<PhysicsWorld3D>>,
                 shoot_tick: i64,
                 ox: f64, oy: f64, oz: f64,
                 dx: f64, dy: f64, dz: f64,
                 max_dist: f64,
                 out_ptr: i32| -> i32 {
                    let world = caller.data().clone();
                    let origin = crate::physics::Vec3::new(ox, oy, oz);
                    let dir = crate::physics::Vec3::new(dx, dy, dz);

                    if let Some(hit) = world.lag_compensated_raycast(shoot_tick as u64, origin, dir, max_dist) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 44 <= data.len() {
                                data[offset..offset + 8].copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16].copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24].copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32].copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36].copy_from_slice(&hit.entity_id.to_le_bytes());
                                data[offset + 36..offset + 40].copy_from_slice(&hit.kind.to_le_bytes());
                                data[offset + 40..offset + 44].copy_from_slice(&hit.collider_id.to_le_bytes());
                            }
                        }
                        hit.hit_type as i32
                    } else {
                        0
                    }
                },
            )
            .expect("link env.sys_physics_lag_raycast");

        let mut wasm = RoomLogic::from_module_with_linker(runtime, &linker, wasm_module, physics)
            .expect("instantiate room");
        wasm.on_create(max_clients).expect("wasm on_create");
        Self {
            wasm,
            sessions: HashMap::new(),
            spectators: std::collections::HashSet::new(),
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
    physics: Arc<PhysicsWorld3D>,
    rooms: HashMap<String, GameRoom>,
}

impl GameServer {
    fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
        let physics = self.physics.clone();
        self.rooms.entry(name.to_string()).or_insert_with(|| {
            eprintln!("[server] room '{name}' created with 3D physics world");
            GameRoom::new(&self.runtime, &self.wasm_module, self.max_clients, physics)
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
        // Direct raw binary input from OpenOxide SDK without msgpack overhead
        Some(payload)
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
        .unwrap_or_default();
    let schema_mjs = std::fs::read(
        cfg.sdk_dir
            .join("node_modules/@colyseus/schema/build/index.mjs"),
    )
    .unwrap_or_default();
    let openoxide_js_path = project_dir.join("openoxide.js");
    let openoxide_js = if openoxide_js_path.exists() {
        std::fs::read(&openoxide_js_path).unwrap_or_default()
    } else {
        include_bytes!("../examples/tank-3d/openoxide.js").to_vec()
    };



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

    let mut addrs = vec![
        format!("0.0.0.0:{}", cfg.port).parse::<SocketAddr>().unwrap()
    ];
    if cfg.web_port != cfg.port {
        addrs.push(format!("0.0.0.0:{}", cfg.web_port).parse::<SocketAddr>().unwrap());
    }

    let map_path = project_dir.join("arena_map.json");
    let physics = if map_path.exists() {
        match std::fs::read_to_string(&map_path) {
            Ok(json_str) => match PhysicsWorld3D::from_json(&json_str) {
                Ok(w) => {
                    eprintln!(
                        "[server] 🗺️  loaded arena_map.json with {} static colliders",
                        w.obstacles.len()
                    );
                    Arc::new(w)
                }
                Err(e) => {
                    eprintln!("[server] failed to parse arena_map.json: {e}, using default world");
                    Arc::new(PhysicsWorld3D::default())
                }
            },
            Err(_) => Arc::new(PhysicsWorld3D::default()),
        }
    } else {
        Arc::new(PhysicsWorld3D::default())
    };

    let mut transport = ColyseusWsTransport::new_multi(&addrs)
        .expect("bind transport ports");

    let website_path_clone = website_path.clone();
    let default_website_clone = website.clone();
    let openoxide_js_clone = openoxide_js.clone();
    let sdk_js_clone = sdk_js.clone();
    let schema_mjs_clone = schema_mjs.clone();
    let map_path_clone = map_path.clone();
    let webgl_dir_clone = project_dir
        .parent()
        .map(|p| p.join("OpenOxideArena/Build/WebGL"))
        .unwrap_or_else(|| project_dir.join("unity"));

    transport.on_http(move |raw_path| {
        let path = raw_path.split('?').next().unwrap_or(raw_path);
        if path == "/unity" || path == "/unity/" {
            let index_path = webgl_dir_clone.join("index.html");
            if let Ok(body) = std::fs::read(&index_path) {
                return Some(("text/html; charset=utf-8".to_string(), body));
            }
        }
        if let Some(subpath) = path.strip_prefix("/unity/").or_else(|| {
            if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
                path.strip_prefix("/")
            } else {
                None
            }
        }) {
            let file_path = webgl_dir_clone.join(subpath);
            if file_path.is_file() {
                if let Ok(body) = std::fs::read(&file_path) {
                    let content_type = match file_path.extension().and_then(|s| s.to_str()).unwrap_or("") {
                        "html" => "text/html; charset=utf-8",
                        "js" => "application/javascript",
                        "wasm" => "application/wasm",
                        "data" => "application/octet-stream",
                        "css" => "text/css",
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "json" => "application/json",
                        _ => "application/octet-stream",
                    };
                    return Some((content_type.to_string(), body));
                }
            }
        }

        match path {
            "/health" => Some(("text/plain; charset=utf-8".to_string(), b"ok".to_vec())),
            "/" | "/website.html" => {
                let body = std::fs::read(&website_path_clone)
                    .unwrap_or_else(|_| default_website_clone.clone());
                Some(("text/html; charset=utf-8".to_string(), body))
            }
            "/openoxide.js" => {
                Some(("text/javascript; charset=utf-8".to_string(), openoxide_js_clone.clone()))
            }
            "/arena_map.json" => {
                let body = std::fs::read(&map_path_clone).unwrap_or_default();
                Some(("application/json".to_string(), body))
            }
            "/packages/sdk/dist/colyseus.js" => {
                Some(("text/javascript".to_string(), sdk_js_clone.clone()))
            }
            "/node_modules/@colyseus/schema/build/index.mjs" => {
                Some(("text/javascript".to_string(), schema_mjs_clone.clone()))
            }
            _ => None,
        }
    });

    for addr in transport.bound_addrs() {
        eprintln!(
            "[server] listening on http://0.0.0.0:{} (website) & ws://0.0.0.0:{} (game)",
            addr.port(),
            addr.port()
        );
    }

    // Direct UDP Listener for Unity / Unreal / Godot (Port 7778)
    let udp_socket = std::net::UdpSocket::bind("0.0.0.0:7778").ok();
    if let Some(ref s) = udp_socket {
        let _ = s.set_nonblocking(true);
        eprintln!("[server] listening on udp://0.0.0.0:7778 (Unity / Unreal / Godot Native Engine)");
    }
    let mut udp_peers: HashMap<SocketAddr, (i64, Instant)> = HashMap::new();

    let wasm_module = runtime.compile(&wasm_bytes).expect("compile room");
    let server = Arc::new(Mutex::new(GameServer {
        runtime: runtime.clone(),
        wasm_module,
        max_clients: cfg.max_clients,
        physics: physics.clone(),
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
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();
                    let is_spectator = raw_room.contains("spectator");
                    let room_name = raw_room.split('?').next().unwrap_or(&raw_room).to_string();

                    let mut server = server.lock().unwrap();
                    let room = server.get_or_create(&room_name);
                    room.empty_since = None;

                    if is_spectator {
                        room.spectators.insert(client_id.clone());
                        eprintln!("[server] 👁️  {client_id} connected as SPECTATOR to room '{room_name}'");
                    } else {
                        let (x, y) = {
                            let n = room.sessions.len();
                            (
                                60.0 + ((n as f64) * 97.0) % (cfg.width - 120.0),
                                60.0 + ((n as f64) * 61.0) % (cfg.height - 120.0),
                            )
                        };
                        let id = room.next_player_id;
                        room.next_player_id += 1;
                        room.wasm.on_join(id, x, y).expect("wasm on_join");
                        room.sessions.insert(client_id.clone(), id);
                        eprintln!("[server] {client_id} joined room '{room_name}' (player id {id})");
                    }
                    let _ = transport.flush();
                }
                TransportEvent::Disconnected { client_id } => {
                    eprintln!("[server] disconnected: {client_id}");
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();
                    let room_name = raw_room.split('?').next().unwrap_or(&raw_room).to_string();
                    let mut server = server.lock().unwrap();
                    let Some(room) = server.rooms.get_mut(&room_name) else {
                        continue;
                    };
                    room.spectators.remove(&client_id);
                    if let Some(id) = room.sessions.remove(&client_id) {
                        let _ = room.wasm.on_leave(id);
                    }
                    if room.sessions.is_empty() && room.spectators.is_empty() {
                        room.empty_since = Some(Instant::now());
                    }
                }
                TransportEvent::Packet {
                    client_id, payload, ..
                } => {
                    // Raw bytes pass-through: strip the msgpack type header,
                    // copy into wasm memory, hand over the pointer.
                    let Some(data) = strip_type_header(&payload) else {
                        continue;
                    };
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name)
                        .to_string();
                    let room_name = raw_room.split('?').next().unwrap_or(&raw_room).to_string();
                    let mut server = server.lock().unwrap();
                    let Some(room) = server.rooms.get_mut(&room_name) else {
                        continue;
                    };

                    // Spectator Upgrade Handshake: [0xFF, 'S', 'P', 'E', 'C']
                    if data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC" {
                        if let Some(id) = room.sessions.remove(&client_id) {
                            let _ = room.wasm.on_leave(id);
                        }
                        room.spectators.insert(client_id.clone());
                        eprintln!("[server] 👁️  {client_id} upgraded to SPECTATOR in room '{room_name}'");
                        let confirm = wrap_delta(0, &[]);
                        let _ = transport.send_to(&client_id, OpCode::RoomDataBytes, &confirm);
                        continue;
                    }

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

        // Poll UDP socket for Unity / Unreal / Godot incoming datagrams
        if let Some(ref udp_sock) = udp_socket {
            let mut udp_buf = [0u8; 4096];
            while let Ok((amt, peer_addr)) = udp_sock.recv_from(&mut udp_buf) {
                let data = &udp_buf[..amt];
                let mut server_guard = server.lock().unwrap();
                let room = server_guard.get_or_create(&cfg.name);
                room.empty_since = None;

                let is_spec_handshake = data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC";

                if !udp_peers.contains_key(&peer_addr) {
                    if is_spec_handshake {
                        udp_peers.insert(peer_addr, (0, Instant::now())); // 0 = spectator
                        eprintln!("[server-udp] 👁️  {peer_addr} connected as SPECTATOR via UDP");
                        let confirm = wrap_delta(0, &[]);
                        let _ = udp_sock.send_to(&confirm, peer_addr);
                    } else {
                        let id = room.next_player_id;
                        room.next_player_id += 1;
                        let n = room.sessions.len() + udp_peers.len();
                        let (x, y) = (
                            60.0 + ((n as f64) * 97.0) % (cfg.width - 120.0),
                            60.0 + ((n as f64) * 61.0) % (cfg.height - 120.0),
                        );
                        let _ = room.wasm.on_join(id, x, y);
                        udp_peers.insert(peer_addr, (id, Instant::now()));
                        eprintln!("[server-udp] {peer_addr} joined room via UDP (player id {id})");

                        if data.len() == 20 {
                            if let Ok(ptr) = room.wasm.alloc(data.len() as i32) {
                                let _ = room.wasm.write_memory(ptr as u32, data);
                                let _ = room.wasm.on_input(id, ptr, data.len() as i32);
                            }
                        }
                    }
                } else if let Some((id, last_seen)) = udp_peers.get_mut(&peer_addr) {
                    *last_seen = Instant::now();
                    if is_spec_handshake && *id != 0 {
                        let _ = room.wasm.on_leave(*id);
                        *id = 0;
                        eprintln!("[server-udp] 👁️  {peer_addr} upgraded to SPECTATOR via UDP");
                    } else if *id > 0 && !data.is_empty() {
                        if let Ok(ptr) = room.wasm.alloc(data.len() as i32) {
                            let _ = room.wasm.write_memory(ptr as u32, data);
                            let _ = room.wasm.on_input(*id, ptr, data.len() as i32);
                        }
                    }
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

                    // 1. Broadcast to active players with their player ID
                    for (session, id) in &room.sessions {
                        let frame = wrap_delta(*id, &delta);
                        let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
                    }

                    // 2. Broadcast to spectators with ID 0 (Observer mode)
                    if !room.spectators.is_empty() {
                        let spec_frame = wrap_delta(0, &delta);
                        for spec_session in &room.spectators {
                            let _ = transport.send_to(spec_session, OpCode::RoomDataBytes, &spec_frame);
                        }
                    }

                    // 3. Broadcast to UDP Unity / Unreal / Godot peers
                    if let Some(ref udp_sock) = udp_socket {
                        for (&peer_addr, &(id, _)) in &udp_peers {
                            let frame = wrap_delta(id, &delta);
                            let _ = udp_sock.send_to(&frame, peer_addr);
                        }
                    }
                }

                if room.sessions.is_empty() && room.spectators.is_empty() && udp_peers.is_empty() {
                    match room.empty_since {
                        Some(t) if t.elapsed() > Duration::from_secs(30) => {
                            dead_rooms.push(name.clone());
                        }
                        Some(_) => {}
                        None => room.empty_since = Some(Instant::now()),
                    }
                }
            }

            // Cleanup inactive UDP peers
            let now = Instant::now();
            let mut timed_out_udp = Vec::new();
            udp_peers.retain(|&addr, (id, last_seen)| {
                if now.duration_since(*last_seen) > Duration::from_secs(10) {
                    timed_out_udp.push((addr, *id));
                    false
                } else {
                    true
                }
            });
            for (addr, id) in timed_out_udp {
                eprintln!("[server-udp] disconnected (timeout): {addr}");
                if id > 0 {
                    if let Some(room) = server.rooms.get_mut(&cfg.name) {
                        let _ = room.wasm.on_leave(id);
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
