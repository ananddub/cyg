//! # ox-game server — Generic Byte Pipeline
//!
//! PS5 vs Game CD architecture: the Rust engine knows NOTHING about the
//! game. The WASM room owns all state and its own wire encoding. The host
//! is a dumb-but-fast pipe:
//!
//!   browser input  →  wasm.on_input(id, ptr, len)   (raw bytes pass-through)
//!   20-128 Hz      →  wasm.on_tick(dt) -> packed(ptr, len)
//!                     host reads delta bytes from wasm memory
//!                  →  broadcast [ROOM_DATA_BYTES]["delta" + bytes] to clients
//!
//! Hot-reload (Level 1): when `room_logic.wasm` changes on disk, new rooms
//! are served by the new module. Existing rooms finish on the old one —
//! no client disconnects.

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};


use engine_net::framing::OpCode;
use engine_net::transport::{
    ColyseusWsTransport, GameTransport, TransportEvent, WebRtcServer, WebRtcServerEvent,
};
use engine_wasm::{WasmRuntime, wasm_module};
use crate::physics::PhysicsWorld3D;

// Type-safe wrapper over the compiled room WASM. The host never opens the
// game's bytes — it only passes pointers and reads back the delta buffer.
wasm_module! {
    pub struct RoomLogic {
        fn on_create(max: i32);
        fn on_join(id: i64, x: f64, y: f64);
        fn on_leave(id: i64);
        fn on_alloc(len: i32) -> i32;
        fn on_input(id: i64, ptr: i32, len: i32);
        fn on_tick(dt: f64) -> i64;
        fn on_save() -> i64;
        fn on_load(ptr: i32, len: i32);
    }
}

/// One named room: an isolated WASM simulation + its members.
struct GameRoom {
    wasm: RoomLogic<PhysicsWorld3D>,
    sessions: HashMap<String, i64>,
    spectators: std::collections::HashSet<String>,
    next_player_id: i64,
    empty_since: Option<Instant>,
    current_tick: u64,
    rollback_buffer: engine_wasm::RollbackRingBuffer,
    voice_room: engine_voice::VoiceRoom,
    /// WebRTC signaling membership: which voice peers exist and which socket each
    /// one is carried on. Separate from `voice_room` because a peer opts into the
    /// P2P mesh with a `Join` signal, while `voice_room` tracks everyone.
    voice_mesh: engine_voice::VoiceMesh,
    last_input_seq: HashMap<i64, u16>,
    event_seq: u16,
}

/// Build the host-import linker once at startup. The ~12 `func_wrap` closures
/// are identical for every room, so building them per spawn was pure waste;
/// pre-instantiating against this linker makes each new room a single
/// `InstancePre::instantiate`.
fn build_host_linker(
    runtime: &WasmRuntime,
    permissions: engine_wasm::AbiPermissions,
) -> wasmtime::Linker<PhysicsWorld3D> {
    let mut linker = runtime.create_linker::<PhysicsWorld3D>();

    // Register host functions under both canonical `game_host_v1` and legacy `env` namespaces (Rule 1 & Rule 8)
    for ns in &[engine_wasm::HOST_NAMESPACE_V1, engine_wasm::HOST_NAMESPACE_LEGACY] {
        // Abort callback
        linker
            .func_wrap(
                ns,
                "abort",
                |_caller: wasmtime::Caller<'_, PhysicsWorld3D>, _m: u32, _f: u32, _l: u32, _c: u32| {},
            )
            .expect("link abort");

        // Host Log ABI: UTF-8 validated, bounds-checked (Rule 2 & Rule 4)
        // Host Log ABI: UTF-8 validated, bounds-checked (Rule 2 & Rule 4)
        linker
            .func_wrap(
                ns,
                "host_log",
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>, ptr: i32, len: i32| -> i32 {
                    if !permissions.logging {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if len <= 0 {
                        return engine_wasm::ABI_OK;
                    }
                    let len = len as usize;
                    if len > engine_wasm::MAX_LOG_LENGTH {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data(&caller);
                        let offset = ptr as usize;
                        if offset + len <= data.len() {
                            if let Ok(msg) = std::str::from_utf8(&data[offset..offset + len]) {
                                eprintln!("[wasm-log] {msg}");
                                return engine_wasm::ABI_OK;
                            } else {
                                return engine_wasm::ABI_ERR_INVALID_UTF8;
                            }
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link host_log");

        // Host Version ABI (Rule 1: Version query)
        linker
            .func_wrap(
                ns,
                "get_abi_version",
                move |_caller: wasmtime::Caller<'_, PhysicsWorld3D>| -> i32 {
                    engine_wasm::CURRENT_ABI_VERSION as i32
                },
            )
            .expect("link get_abi_version");

        // Host Physics ABI 1: Surface height query (for solid standing & rooftops)
        linker
            .func_wrap(
                ns,
                "sys_physics_surface_y",
                move |caller: wasmtime::Caller<'_, PhysicsWorld3D>, px: f64, py: f64, pz: f64, radius: f64| -> f64 {
                    if !permissions.physics {
                        return 0.0;
                    }
                    caller.data().get_surface_height(px, py, pz, radius)
                },
            )
            .expect("link sys_physics_surface_y");

        // Host Physics ABI 2: Kinematic move_and_slide (smooth wall sliding along obstacle normals)
        linker
            .func_wrap(
                ns,
                "sys_physics_move",
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>,
                 px: f64, py: f64, pz: f64,
                 vx: f64, vy: f64, vz: f64,
                 radius: f64, dt: f64,
                 out_ptr: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let res = caller.data().move_and_slide(px, py, pz, vx, vy, vz, radius, dt);

                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 24 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&res.new_x.to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&res.new_y.to_le_bytes());
                            data[offset + 16..offset + 24].copy_from_slice(&res.new_z.to_le_bytes());
                        } else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                    }

                    if res.is_grounded { 1 } else { 0 }
                },
            )
            .expect("link sys_physics_move");

        // Host Physics ABI 3: Continuous Collision Detection Raycasting (bullets & hitscan)
        linker
            .func_wrap(
                ns,
                "sys_physics_raycast",
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>,
                 ox: f64, oy: f64, oz: f64,
                 dx: f64, dy: f64, dz: f64,
                 max_dist: f64,
                 out_ptr: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = crate::physics::Vec3::new(ox, oy, oz);
                    let dir = crate::physics::Vec3::new(dx, dy, dz);

                    let res = if let Some(hit) = caller.data().raycast(origin, dir, max_dist) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 36 <= data.len() {
                                data[offset..offset + 8].copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16].copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24].copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32].copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36].copy_from_slice(&hit.collider_id.to_le_bytes());
                            } else {
                                return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                            }
                        }
                        1
                    } else {
                        0
                    };
                    res
                },
            )
            .expect("link sys_physics_raycast");

        // Host Physics ABI 4: Lag Compensation Entity Hitbox Recording
        linker
            .func_wrap(
                ns,
                "sys_physics_lag_record",
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>,
                 tick: i64,
                 entity_id: i32,
                 kind: i32,
                 min_x: f64, min_y: f64, min_z: f64,
                 max_x: f64, max_y: f64, max_z: f64| {
                    if !permissions.lag_compensation {
                        return;
                    }
                    let bounds = crate::physics::Aabb {
                        min: crate::physics::Vec3::new(min_x, min_y, min_z),
                        max: crate::physics::Vec3::new(max_x, max_y, max_z),
                    };
                    caller.data_mut().record_entity_hitbox(
                        tick as u64,
                        0,
                        entity_id as u32,
                        kind as u32,
                        bounds,
                    );
                },
            )
            .expect("link sys_physics_lag_record");

        // Host Physics ABI 5: Lag-Compensated Rewind Raycast (reference Fusion Server Rewind)
        linker
            .func_wrap(
                ns,
                "sys_physics_lag_raycast",
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>,
                 shoot_tick: i64,
                 ox: f64, oy: f64, oz: f64,
                 dx: f64, dy: f64, dz: f64,
                 max_dist: f64,
                 out_ptr: i32| -> i32 {
                    if !permissions.lag_compensation {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = crate::physics::Vec3::new(ox, oy, oz);
                    let dir = crate::physics::Vec3::new(dx, dy, dz);

                    if let Some(hit) = caller.data_mut().lag_compensated_raycast(shoot_tick as u64, origin, dir, max_dist) {
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
                            } else {
                                return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                            }
                        }
                        hit.hit_type as i32
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_physics_lag_raycast");
    }

    linker
}

impl GameRoom {
    fn new(
        pre: &engine_wasm::WasmPreInstance<PhysicsWorld3D>,
        max_clients: i32,
        physics: PhysicsWorld3D,
    ) -> Self {
        let mut wasm = RoomLogic::from_pre(pre, physics).expect("instantiate room");
        if let Err(e) = wasm.on_create(max_clients) {
            eprintln!("[server] ⚠️ guest on_create error: {e}");
        }
        Self {
            wasm,
            sessions: HashMap::new(),
            spectators: std::collections::HashSet::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
            current_tick: 0,
            rollback_buffer: engine_wasm::RollbackRingBuffer::default(),
            voice_room: engine_voice::VoiceRoom::new(engine_voice::DistanceModel::Inverse {
                min_distance: 10.0,
                max_distance: 250.0,
                rolloff: 1.0,
            }),
            voice_mesh: engine_voice::VoiceMesh::default(),
            last_input_seq: HashMap::new(),
            event_seq: 0,
        }
    }
}

/// All rooms + the pre-instantiated room template (hot-swappable).
struct GameServer {
    /// Linked, validated template: spawning a room costs one instantiate.
    pre: engine_wasm::WasmPreInstance<PhysicsWorld3D>,
    max_clients: i32,
    physics: PhysicsWorld3D,
    rooms: HashMap<String, GameRoom>,
    permissions: engine_wasm::AbiPermissions,
}

impl GameServer {
    fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
        // Hot path for an existing room borrows the key: no String per packet.
        if !self.rooms.contains_key(name) {
            let physics = self.physics.clone();
            let room = GameRoom::new(&self.pre, self.max_clients, physics);
            eprintln!(
                "[server] room '{name}' created with 3D physics world (ABI v{})",
                engine_wasm::CURRENT_ABI_VERSION
            );
            self.rooms.insert(name.to_string(), room);
        }
        self.rooms.get_mut(name).expect("room just inserted")
    }
}

/// Wrap a raw delta buffer into a Colyseus ROOM_DATA_BYTES frame payload:
/// [msgpack fixstr "delta"][i64 recipient_player_id][raw bytes...].
/// Frames into a caller-owned buffer so a 128 Hz broadcast allocates nothing.
fn wrap_delta_into(out: &mut Vec<u8>, your_id: i64, delta: &[u8]) {
    out.clear();
    out.reserve(delta.len() + 14);
    out.push(0xA5);
    out.extend_from_slice(b"delta");
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(delta);
}

/// Pre-framed spectator "you are watching" confirm: `["delta"]` with id 0.
const SPECTATOR_CONFIRM: &[u8] = &[0xA5, b'd', b'e', b'l', b't', b'a', 0, 0, 0, 0, 0, 0, 0, 0];

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

/// Tell every voice peer in the mesh, except `subject`, that the roster changed.
///
/// The body is a one-element JSON array so the SDK can read it without the
/// server owning a JSON type. One frame is encoded and shared across the fan-out.
fn announce_voice_peer_change(
    transport: &mut impl GameTransport,
    mesh: &mut engine_voice::VoiceMesh,
    kind: engine_voice::SignalKind,
    subject: u32,
) {
    let frame = engine_voice::SignalFrame::new(
        kind,
        engine_voice::SERVER_SENDER,
        engine_voice::BROADCAST,
        bytes::Bytes::from(format!("[{subject}]")),
    )
    .encode();
    mesh.for_each_peer(subject, |target| {
        let _ = transport.send_to(target.session_id, OpCode::RoomDataBytes, &frame);
    });
}

/// Reject a signaling frame a client is not allowed to send. `to` is the sender:
/// these replies only ever go back to the offender, who then tears down that peer.
fn reject_signal(
    transport: &mut impl GameTransport,
    client_id: &str,
    from: u32,
    reason: &'static str,
) {
    let frame = engine_voice::SignalFrame::new(
        engine_voice::SignalKind::Error,
        engine_voice::SERVER_SENDER,
        from,
        bytes::Bytes::from(format!(r#"{{"reason":"{reason}"}}"#)),
    )
    .encode();
    let _ = transport.send_to(client_id, OpCode::RoomDataBytes, &frame);
}

/// Static files served over the game port. Each path is read from disk once
/// and only re-read when its mtime changes, so a page load with 30 assets
/// touches the filesystem once per changed file instead of once per request.
#[derive(Default)]
struct FileCache {
    files: HashMap<std::path::PathBuf, (Option<std::time::SystemTime>, bytes::Bytes)>,
}

impl FileCache {
    fn load(&mut self, path: &Path) -> Option<bytes::Bytes> {
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        if let Some((seen, body)) = self.files.get(path) {
            if mtime.is_some() && *seen == mtime {
                return Some(body.clone());
            }
        }
        let body = bytes::Bytes::from(std::fs::read(path).ok()?);
        self.files
            .insert(path.to_path_buf(), (mtime, body.clone()));
        Some(body)
    }
}

/// Content type for a WebGL build extension, as a `'static` str so serving
/// never allocates a header value.
fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "json" => "application/json",
        "data" => "application/octet-stream",
        _ => "application/octet-stream",
    }
}

/// Reject `..`/absolute components in a URL-derived relative path before
/// joining it onto the asset directory.
fn safe_relative(sub: &str) -> Option<&Path> {
    let p = Path::new(sub);
    let mut depth = 0usize;
    for c in p.components() {
        match c {
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::ParentDir => return None,
            _ => return None,
        }
    }
    if depth == 0 {
        None
    } else {
        Some(p)
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
                    w
                }
                Err(e) => {
                    eprintln!("[server] failed to parse arena_map.json: {e}, using default world");
                    PhysicsWorld3D::default()
                }
            },
            Err(_) => PhysicsWorld3D::default(),
        }
    } else {
        PhysicsWorld3D::default()
    };

    let mut transport = ColyseusWsTransport::new_multi(&addrs)
        .expect("bind transport ports");

    let webgl_dir = project_dir
        .parent()
        .map(|p| p.join("OpenOxideArena/Build/WebGL"))
        .unwrap_or_else(|| project_dir.join("unity"));
    let website = bytes::Bytes::from(website);
    let openoxide_js = bytes::Bytes::from(openoxide_js);
    let sdk_js = bytes::Bytes::from(sdk_js);
    let schema_mjs = bytes::Bytes::from(schema_mjs);
    let web_cache = std::sync::Arc::new(std::sync::Mutex::new(FileCache::default()));

    let cache_hook = web_cache.clone();
    let website_path_hook = website_path.clone();
    let openoxide_js_path_hook = openoxide_js_path.clone();
    let map_path_hook = map_path.clone();
    let website_default = website.clone();
    let openoxide_default = openoxide_js.clone();
    let sdk_hook = sdk_js.clone();
    let schema_hook = schema_mjs.clone();

    let rooms_meta = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, usize, usize)>::new()));
    let rooms_meta_http = rooms_meta.clone();

    transport.on_http(move |raw_path| {
        let path = raw_path.split('?').next().unwrap_or(raw_path);
        // Uncontended by design: one transport thread serves all static hits.
        let mut cache = cache_hook.lock().ok()?;

        if path == "/unity" || path == "/unity/" {
            if let Some(body) = cache.load(&webgl_dir.join("index.html")) {
                return Some((Cow::Borrowed("text/html; charset=utf-8"), body));
            }
        }
        if let Some(subpath) = path.strip_prefix("/unity/").or_else(|| {
            if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
                path.strip_prefix("/")
            } else {
                None
            }
        }) {
            if let Some(rel) = safe_relative(subpath) {
                let file_path = webgl_dir.join(rel);
                if let Some(body) = cache.load(&file_path) {
                    return Some((Cow::Borrowed(content_type_for(&file_path)), body));
                }
            }
        }

        match path {
            "/api/rooms" => {
                let meta = rooms_meta_http.lock().ok()?;
                let mut json_items = Vec::new();
                for (name, players, specs) in meta.iter() {
                    json_items.push(format!(
                        "{{\"name\":\"{}\",\"players\":{},\"spectators\":{}}}",
                        name, players, specs
                    ));
                }
                let body = format!("[{}]", json_items.join(","));
                Some((
                    Cow::Borrowed("application/json; charset=utf-8"),
                    bytes::Bytes::from(body),
                ))
            }
            "/health" => Some((
                Cow::Borrowed("text/plain; charset=utf-8"),
                bytes::Bytes::from_static(b"ok"),
            )),
            "/" | "/website.html" => Some((
                Cow::Borrowed("text/html; charset=utf-8"),
                cache
                    .load(&website_path_hook)
                    .unwrap_or_else(|| website_default.clone()),
            )),
            "/openoxide.js" => Some((
                Cow::Borrowed("text/javascript; charset=utf-8"),
                cache
                    .load(&openoxide_js_path_hook)
                    .unwrap_or_else(|| openoxide_default.clone()),
            )),
            "/arena_map.json" => Some((
                Cow::Borrowed("application/json"),
                cache.load(&map_path_hook).unwrap_or_default(),
            )),
            "/packages/sdk/dist/colyseus.js" => {
                Some((Cow::Borrowed("text/javascript"), sdk_hook.clone()))
            }
            "/node_modules/@colyseus/schema/build/index.mjs" => {
                Some((Cow::Borrowed("text/javascript"), schema_hook.clone()))
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

    // Direct WebRTC DataChannel UDP Server (Client-to-Server Star Topology via str0m)
    let webrtc_port = 7779;
    let webrtc_udp = std::net::UdpSocket::bind(format!("0.0.0.0:{webrtc_port}"))
        .or_else(|_| std::net::UdpSocket::bind("0.0.0.0:0"))
        .ok();

    let mut webrtc_server = if let Some(ref sock) = webrtc_udp {
        let _ = sock.set_nonblocking(true);
        let actual_port = sock.local_addr().map(|a| a.port()).unwrap_or(webrtc_port);
        eprintln!("[server] ⚡ WebRTC DataChannel server listening on udp://0.0.0.0:{actual_port}");
        let local_addr: SocketAddr = format!("127.0.0.1:{actual_port}").parse().unwrap();
        let mut srv = WebRtcServer::new(vec![local_addr]);
        let lan_ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s.local_addr()))
            .ok()
            .and_then(|a| a.ok())
            .map(|a| a.ip());
        if let Some(ip) = lan_ip {
            let lan_addr = SocketAddr::new(ip, actual_port);
            if lan_addr != local_addr {
                srv.add_announced_addr(lan_addr);
                eprintln!("[server] ⚡ WebRTC announced LAN candidate: {lan_addr}");
            }
        }
        if let Ok(ip) = std::env::var("PUBLIC_IP") {
            if let Ok(a) = format!("{ip}:{actual_port}").parse() {
                srv.add_announced_addr(a);
            }
        }
        Some(srv)
    } else {
        None
    };

    eprintln!(
        "[server] ABI contract: host v{} (manifest requested: v{})",
        engine_wasm::CURRENT_ABI_VERSION,
        cfg.abi_version
    );
    if cfg.abi_version > engine_wasm::CURRENT_ABI_VERSION {
        eprintln!(
            "[server] ⚠️  Warning: manifest requests ABI version {}, but server only supports up to v{}",
            cfg.abi_version,
            engine_wasm::CURRENT_ABI_VERSION
        );
    }

    let wasm_module = runtime.compile(&wasm_bytes).expect("compile room");
    // Link + pre-instantiate once; every room spawn after this is one
    // `InstancePre::instantiate` instead of rebuilding 12 host imports.
    let linker = build_host_linker(&runtime, cfg.permissions);
    let pre = runtime
        .prepare(&linker, &wasm_module)
        .expect("pre-instantiate room module");
    let mut server = GameServer {
        pre,
        max_clients: cfg.max_clients,
        physics,
        rooms: HashMap::new(),
        permissions: cfg.permissions,
    };

    let mut last_tick = Instant::now();
    let tick_interval = Duration::from_secs_f64(1.0 / cfg.tick_rate.max(1) as f64);
    let mut tick_counter: u64 = 0;
    let mut tick_stat_start = Instant::now();
    let mut last_wasm_check = Instant::now();
    let mut loaded_wasm_time: Option<std::time::SystemTime> =
        std::fs::metadata(wasm_path).ok().and_then(|m| m.modified().ok());
    let source_path = project_dir.join(&cfg.source);
    let mut loaded_src_time: Option<std::time::SystemTime> =
        std::fs::metadata(&source_path).ok().and_then(|m| m.modified().ok());
    let mut last_meta_update = Instant::now();
    let mut events = Vec::new();
    // Tick scratch: the delta read and the framed broadcast reuse these.
    let mut delta_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut frame_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut dead_rooms: Vec<String> = Vec::new();
    let mut timed_out_udp: Vec<(SocketAddr, i64)> = Vec::new();

    loop {
        let time_to_next_tick = tick_interval.saturating_sub(last_tick.elapsed());
        let poll_timeout = if time_to_next_tick > Duration::from_millis(1) {
            time_to_next_tick.saturating_sub(Duration::from_millis(1)).min(Duration::from_millis(2))
        } else {
            Duration::ZERO
        };
        transport
            .poll_events(poll_timeout, &mut events)
            .expect("poll");

        for event in events.drain(..) {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name);
                    let is_spectator = raw_room.contains("spectator");
                    let room_name = raw_room.split('?').next().unwrap_or(raw_room);

                    let room = server.get_or_create(room_name);
                    room.empty_since = None;

                    if is_spectator {
                        room.spectators.insert(client_id.to_string());
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
                        if let Err(e) = room.wasm.on_join(id, x, y) {
                            eprintln!("[server] ⚠️ guest on_join error for player {id}: {e}");
                        }
                        room.voice_room.join_participant(id as u32, Some(1), [x, 0.0, y]);
                        room.sessions.insert(client_id.to_string(), id);
                        eprintln!("[server] {client_id} joined room '{room_name}' (player id {id})");
                    }
                    let _ = transport.flush();
                }
                TransportEvent::Disconnected { client_id } => {
                    eprintln!("[server] disconnected: {client_id}");
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name);
                    let room_name = raw_room.split('?').next().unwrap_or(raw_room);
                    let Some(room) = server.rooms.get_mut(room_name) else {
                        continue;
                    };
                    room.spectators.remove(&*client_id);
                    // A WebRTC peer that vanishes with its socket has to be
                    // announced, otherwise the rest of the mesh keeps
                    // re-negotiating with an id nobody answers.
                    if let Some(gone) = room.voice_mesh.leave_session(&client_id) {
                        announce_voice_peer_change(
                            &mut transport,
                            &mut room.voice_mesh,
                            engine_voice::SignalKind::PeerLeft,
                            gone,
                        );
                    }
                    if let Some(id) = room.sessions.remove(&*client_id) {
                        let _ = room.wasm.on_leave(id);
                        room.voice_room.leave_participant(id as u32);
                        room.last_input_seq.remove(&id);
                    }
                    if let Some(ref mut srv) = webrtc_server {
                        srv.remove_session(&client_id);
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
                    // Rule 4: Bounds validation on inbound payload
                    if data.len() > engine_wasm::MAX_INBOUND_PAYLOAD_SIZE {
                        eprintln!(
                            "[server] ⚠️ rejected payload from {client_id}: size {} exceeds max {}",
                            data.len(),
                            engine_wasm::MAX_INBOUND_PAYLOAD_SIZE
                        );
                        continue;
                    }
                    let raw_room = transport
                        .room_of(&client_id)
                        .unwrap_or(&cfg.name);
                    let room_name = raw_room.split('?').next().unwrap_or(raw_room);
                    let Some(room) = server.rooms.get_mut(room_name) else {
                        continue;
                    };

                    // Spectator Upgrade Handshake: [0xFF, 'S', 'P', 'E', 'C']
                    if data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC" {
                        if let Some(id) = room.sessions.remove(&*client_id) {
                            let _ = room.wasm.on_leave(id);
                        }
                        room.spectators.insert(client_id.to_string());
                        eprintln!("[server] 👁️  {client_id} upgraded to SPECTATOR in room '{room_name}'");
                        let _ = transport.send_to(&client_id, OpCode::RoomDataBytes, SPECTATOR_CONFIRM);
                        continue;
                    }

                    let Some(&id) = room.sessions.get(&*client_id) else {
                        continue;
                    };

                    // WebRTC signaling: ['W'][version][kind][from u32][to u32][JSON]
                    // The host never opens the SDP/ICE body. It stamps the sender
                    // from its own session map, checks the target is a live peer,
                    // and forwards the frame — the media itself goes peer-to-peer.
                    if engine_voice::SignalFrame::is_signal(data) {
                        // Zero-copy view of the bytes the frame actually arrived in.
                        let wire = payload.slice(payload.len() - data.len()..);
                        let Ok(signal) = engine_voice::SignalFrame::decode(&wire) else {
                            eprintln!("[server] ⚠️ dropped malformed signal from {client_id}");
                            continue;
                        };
                        let from = id as u32;

                        match signal.kind {
                            engine_voice::SignalKind::Join => {
                                // The joiner gets the ICE list, its own authoritative
                                // id and the peers it can reach; everyone else gets
                                // told a peer appeared. `self` is what lets the client
                                // apply the lower-id-creates-the-offer rule without
                                // guessing its id from replicated state.
                                let mut body = String::from(r#"{"iceServers":"#);
                                body.push_str(&cfg.voice_ice_servers);
                                body.push_str(r#","peers":["#);
                                let peers = room.voice_mesh.peers_sorted();
                                for (i, &peer) in peers.iter().enumerate() {
                                    if i > 0 {
                                        body.push(',');
                                    }
                                    body.push_str(&peer.to_string());
                                }
                                body.push_str(r#"],"self":"#);
                                body.push_str(&from.to_string());
                                body.push('}');

                                let hello = engine_voice::SignalFrame::new(
                                    engine_voice::SignalKind::Config,
                                    engine_voice::SERVER_SENDER,
                                    from,
                                    bytes::Bytes::from(body),
                                )
                                .encode();
                                let _ = transport
                                    .send_to(&client_id, OpCode::RoomDataBytes, &hello);

                                if room.voice_mesh.join(from, &client_id) {
                                    announce_voice_peer_change(
                                        &mut transport,
                                        &mut room.voice_mesh,
                                        engine_voice::SignalKind::PeerJoined,
                                        from,
                                    );
                                }
                            }
                            engine_voice::SignalKind::Bye => {
                                if room.voice_mesh.leave(from).is_some() {
                                    announce_voice_peer_change(
                                        &mut transport,
                                        &mut room.voice_mesh,
                                        engine_voice::SignalKind::PeerLeft,
                                        from,
                                    );
                                }
                            }
                            _kind if signal.to == engine_voice::SERVER_SENDER => {
                                if let Some(ref mut webrtc) = webrtc_server {
                                    match signal.kind {
                                        engine_voice::SignalKind::Offer => {
                                            let sdp_str = if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&signal.payload) {
                                                v.get("sdp").and_then(|s| s.as_str()).map(|s| s.to_string())
                                            } else {
                                                std::str::from_utf8(&signal.payload).ok().map(|s| s.to_string())
                                            };
                                            if let Some(sdp) = sdp_str {
                                                match webrtc.handle_offer(&client_id, &sdp) {
                                                    Ok(answer_sdp) => {
                                                        let answer_body = serde_json::json!({
                                                            "sdp": answer_sdp,
                                                            "type": "answer"
                                                        }).to_string();
                                                        let ans_frame = engine_voice::SignalFrame::new(
                                                            engine_voice::SignalKind::Answer,
                                                            engine_voice::SERVER_SENDER,
                                                            from,
                                                            bytes::Bytes::from(answer_body),
                                                        ).encode();
                                                        let _ = transport.send_to(&client_id, OpCode::RoomDataBytes, &ans_frame);
                                                        eprintln!("[server] ⚡ WebRTC Client-Server offer accepted for {client_id}, SDP answer returned");
                                                    }
                                                    Err(e) => {
                                                        eprintln!("[server] ⚠️ WebRTC handle_offer error for client {client_id}: {e}");
                                                    }
                                                }
                                            }
                                        }
                                        engine_voice::SignalKind::Ice => {
                                            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&signal.payload) {
                                                if let Some(cand) = v.get("candidate").and_then(|c| c.as_str()) {
                                                    match webrtc.handle_candidate(&client_id, cand) {
                                                        Ok(_) => eprintln!("[server] 🧊 Client {client_id} added ICE candidate: {cand}"),
                                                        Err(e) => eprintln!("[server] ⚠️ Client {client_id} candidate error: {e} ({cand})"),
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            kind if kind.is_relayed() => {
                                // Point-to-point only: relaying a broadcast offer
                                // would amplify one inbound packet into the room.
                                if signal.to == engine_voice::BROADCAST
                                    || signal.to == from
                                    || !room.voice_mesh.contains(signal.to)
                                {
                                    reject_signal(
                                        &mut transport,
                                        &client_id,
                                        from,
                                        "unknown voice peer target",
                                    );
                                    continue;
                                }
                                let out = engine_voice::rewrite_sender(&wire, from);
                                room.voice_mesh.route(from, signal.to, |target| {
                                    let _ = transport.send_to(
                                        target.session_id,
                                        OpCode::RoomDataBytes,
                                        &out,
                                    );
                                });
                            }
                            other => {
                                eprintln!(
                                    "[server] ⚠️ {client_id} sent a server-only signal ({other:?})"
                                );
                            }
                        }
                        continue;
                    }

                    // Voice Packet Dispatch: [0x56, ...]
                    if data.len() >= engine_voice::HEADER_LEN && data[0] == engine_voice::VOICE_MAGIC {
                        let bytes = bytes::Bytes::copy_from_slice(data);
                        if let Ok(mut packet) = engine_voice::VoicePacket::decode(&bytes) {
                            packet.speaker_id = id as u32;
                            let routed = room.voice_room.dispatch_packet(&packet, Instant::now());
                            let mut wire_packet = data.to_vec();
                            wire_packet[1..5].copy_from_slice(&(id as u32).to_be_bytes());
                            for frame in routed {
                                for (cid, &pid) in &room.sessions {
                                    if pid == id {
                                        continue; // NEVER echo voice packets back to the originating speaker
                                    }
                                    if pid as u32 == frame.listener_id {
                                        let _ = transport.send_to(cid, OpCode::RoomDataBytes, &wire_packet);
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    // Voice Settings (Team / Mute / Deafen): [0x56, 'T', team_id], [0x56, 'M', muted], [0x56, 'D', deafened]
                    if data.len() == 3 && data[0] == engine_voice::VOICE_MAGIC {
                        if data[1] == b'T' {
                            let team_id = data[2] as u16;
                            room.voice_room.set_team(id as u32, Some(team_id));
                            eprintln!("[server] 🛡️ Player {id} switched to Squad {team_id}");
                        } else if data[1] == b'M' {
                            let muted = data[2] == 1;
                            room.voice_room.set_muted(id as u32, muted);
                            eprintln!("[server] 🎙️ Player {id} mic muted={muted}");
                            if room.voice_mesh.peer_count() > 1 {
                                let note = engine_voice::SignalFrame::new(
                                    engine_voice::SignalKind::Muted,
                                    engine_voice::SERVER_SENDER,
                                    engine_voice::BROADCAST,
                                    bytes::Bytes::from(format!("[{id},{}]", muted as u8)),
                                )
                                .encode();
                                room.voice_mesh.for_each_peer(id as u32, |target| {
                                    let _ = transport.send_to(
                                        target.session_id,
                                        OpCode::RoomDataBytes,
                                        &note,
                                    );
                                });
                            }
                        } else if data[1] == b'D' {
                            let deafened = data[2] == 1;
                            room.voice_room.set_deafened(id as u32, deafened);
                            eprintln!("[server] 🎧 Player {id} deafened={deafened}");
                        }
                        continue;
                    }

                    // QUIC-Style Ordered Event Frame: [0x03, seq_u16, kind_u8, from_u16, target_u16, tick_u16, value_f32] (14 bytes)
                    if data.len() == engine_webrtc_protocol::EVENT_WIRE_LEN && data[0] == engine_webrtc_protocol::OP_EVENT {
                        if let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(data) {
                            room.event_seq = room.event_seq.wrapping_add(1);
                            event.seq = room.event_seq;
                            event.from = id as u16; // Server-authoritative sender stamp (Anti-spoofing)

                            let mut wire_event = Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
                            engine_webrtc_protocol::encode_event(&event, &mut wire_event);

                            // SFU Multicast: forward guaranteed event to all other clients in the room
                            for (cid, &pid) in &room.sessions {
                                if pid != id {
                                    let _ = transport.send_to(cid, OpCode::RoomDataBytes, &wire_event);
                                }
                            }
                            if !room.spectators.is_empty() {
                                for spec in &room.spectators {
                                    let _ = transport.send_to(spec, OpCode::RoomDataBytes, &wire_event);
                                }
                            }
                        }
                        continue;
                    }

                    // QUIC-Style Unordered Movement Frame: [0x01, seq_u16, flags_u32, yaw_f32, pitch_f32, mx_f32, mz_f32] (23 bytes)
                    if data.len() == engine_webrtc_protocol::INPUT_WIRE_LEN && data[0] == engine_webrtc_protocol::OP_INPUT {
                        if let Ok(input) = engine_webrtc_protocol::InputFrame::decode(data) {
                            if let Some(ref srv) = webrtc_server {
                                if srv.is_connected(&client_id) && input.seq % 120 == 1 {
                                    eprintln!("[server] ℹ️ Player {id} sent input over WebSocket fallback (seq {})", input.seq);
                                }
                            }
                            let last_seq = room.last_input_seq.entry(id).or_insert(0);
                            // Newest-Wins auto-sync rule (QUIC datagram behavior: drop stale packets, zero HOL blocking)
                            if engine_webrtc_protocol::is_newer(*last_seq, input.seq) || *last_seq == 0 {
                                *last_seq = input.seq;
                                let payload_20 = &data[3..23];
                                if let Ok(ptr_i32) = room.wasm.on_alloc(20) {
                                    let _ = room.wasm.write_memory(ptr_i32 as u32, payload_20);
                                    let _ = room.wasm.on_input(id, ptr_i32, 20);
                                }
                            }
                        }
                        continue;
                    }

                    // Rule 3 (Ownership) & Rule 5 (Fault isolation):
                    // Guest allocates memory; host writes and invokes on_input safely without panicking.
                    match room.wasm.on_alloc(data.len() as i32) {
                        Ok(ptr_i32) => {
                            let ptr = ptr_i32 as u32;
                            if let Err(e) = room.wasm.write_memory(ptr, data) {
                                eprintln!("[server] ⚠️ failed to write memory for player {id}: {e}");
                            } else if let Err(e) = room.wasm.on_input(id, ptr_i32, data.len() as i32) {
                                eprintln!("[server] ⚠️ guest trap in on_input for player {id}: {e}");
                            }
                        }
                        Err(e) => {
                            eprintln!("[server] ⚠️ guest on_alloc failed for player {id}: {e}");
                        }
                    }
                }
            }
        }

        // Poll UDP socket for Unity / Unreal / Godot incoming datagrams
        if let Some(ref udp_sock) = udp_socket {
            let mut udp_buf = [0u8; 4096];
            while let Ok((amt, peer_addr)) = udp_sock.recv_from(&mut udp_buf) {
                let data = &udp_buf[..amt];
                let server_guard = &mut server;
                let room = server_guard.get_or_create(&cfg.name);
                room.empty_since = None;

                let is_spec_handshake = data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC";

                if !udp_peers.contains_key(&peer_addr) {
                    if is_spec_handshake {
                        udp_peers.insert(peer_addr, (0, Instant::now())); // 0 = spectator
                        eprintln!("[server-udp] 👁️  {peer_addr} connected as SPECTATOR via UDP");
                        let _ = udp_sock.send_to(SPECTATOR_CONFIRM, peer_addr);
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
                            if let Ok(ptr) = room.wasm.on_alloc(data.len() as i32) {
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
                        if let Ok(ptr) = room.wasm.on_alloc(data.len() as i32) {
                            let _ = room.wasm.write_memory(ptr as u32, data);
                            let _ = room.wasm.on_input(*id, ptr, data.len() as i32);
                        }
                    }
                }
            }
        }

        // Poll WebRTC UDP socket for Client-to-Server DataChannel traffic (str0m)
        if let Some(ref sock) = webrtc_udp {
            let mut buf = [0u8; 2048];
            let local_addr = sock.local_addr().unwrap_or_else(|_| "0.0.0.0:7779".parse().unwrap());
            while let Ok((amt, peer_addr)) = sock.recv_from(&mut buf) {
                if let Some(ref mut srv) = webrtc_server {
                    srv.handle_udp(peer_addr, local_addr, &buf[..amt]);
                }
            }
        }

        if let Some(ref mut srv) = webrtc_server {
            srv.tick(Instant::now());

            if let Some(ref sock) = webrtc_udp {
                for (dest, data) in srv.poll_transmits() {
                    if let Err(e) = sock.send_to(&data, dest) {
                        eprintln!("[server] ⚠️ WebRTC UDP send_to({dest}) error: {e}");
                    }
                }
            }

            for ev in srv.poll_events() {
                match ev {
                    WebRtcServerEvent::Connected { client_id } => {
                        eprintln!("[server] 🚀 Client {client_id} established WebRTC UDP DataChannel with Server!");
                    }
                    WebRtcServerEvent::ChannelOpen { client_id, label } => {
                        eprintln!("[server] ⚡ WebRTC DataChannel '{label}' open for client {client_id}");
                    }
                    WebRtcServerEvent::Message { client_id, channel_label: _, data } => {
                        let room = server.get_or_create(&cfg.name);
                        let Some(&id) = room.sessions.get(&*client_id) else { continue; };

                        // 1. Unreliable 50Hz Movement Input: [0x01, ...]
                        if data.len() == engine_webrtc_protocol::INPUT_WIRE_LEN && data[0] == engine_webrtc_protocol::OP_INPUT {
                            if let Ok(input) = engine_webrtc_protocol::InputFrame::decode(&data) {
                                if input.seq % 120 == 1 {
                                    eprintln!("[server] ⚡ WebRTC UDP input stream active for player {id} (seq {})", input.seq);
                                }
                                let last_seq = room.last_input_seq.entry(id).or_insert(0);
                                if (input.flags & 16) != 0 {
                                }
                                if engine_webrtc_protocol::is_newer(*last_seq, input.seq) || *last_seq == 0 {
                                    *last_seq = input.seq;
                                    let payload_20 = &data[3..23];
                                    if let Ok(ptr_i32) = room.wasm.on_alloc(20) {
                                        let _ = room.wasm.write_memory(ptr_i32 as u32, payload_20);
                                        let _ = room.wasm.on_input(id, ptr_i32, 20);
                                    }
                                }
                            }
                        }
                        // 2. Reliable Ordered Event Frame: [0x03, ...]
                        else if data.len() == engine_webrtc_protocol::EVENT_WIRE_LEN && data[0] == engine_webrtc_protocol::OP_EVENT {
                            if let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(&data) {
                                room.event_seq = room.event_seq.wrapping_add(1);
                                event.seq = room.event_seq;
                                event.from = id as u16;

                                let mut wire_event = Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
                                engine_webrtc_protocol::encode_event(&event, &mut wire_event);

                                if event.kind == engine_webrtc_protocol::EventKind::Fire {
                                    let fire_payload = 16u32.to_le_bytes();
                                    if let Ok(ptr) = room.wasm.on_alloc(4) {
                                        let _ = room.wasm.write_memory(ptr as u32, &fire_payload);
                                        let _ = room.wasm.on_input(id, ptr, 4);
                                    }
                                }

                                // Broadcast event over reliable WebRTC DataChannel to everyone else
                                srv.broadcast_reliable(&wire_event);

                                // Also forward to WebSocket clients that haven't transitioned yet
                                for (cid, &pid) in &room.sessions {
                                    if pid != id && !srv.is_connected(cid) {
                                        let _ = transport.send_to(cid, OpCode::RoomDataBytes, &wire_event);
                                    }
                                }
                            }
                        }
                    }
                    WebRtcServerEvent::Disconnected { client_id } => {
                        eprintln!("[server] WebRTC disconnected for client {client_id}");
                    }
                }
            }
        }

        // Microsecond precision: spin-wait final tail to hit exact tick deadline without scheduler jitter
        let remaining_to_tick = tick_interval.saturating_sub(last_tick.elapsed());
        if !remaining_to_tick.is_zero() && remaining_to_tick <= Duration::from_micros(600) {
            while last_tick.elapsed() < tick_interval {
                std::hint::spin_loop();
            }
        }

        // ---- fixed-tick: wasm simulates, host broadcasts the delta buffer ----
        let elapsed = last_tick.elapsed();
        if elapsed >= tick_interval {
            let mut ticks_to_run = (elapsed.as_secs_f64() / tick_interval.as_secs_f64()) as u32;
            if ticks_to_run > 3 {
                ticks_to_run = 3;
                last_tick = Instant::now();
            } else {
                last_tick += tick_interval * ticks_to_run;
            }
            let dt = tick_interval.as_secs_f64();

            tick_counter += ticks_to_run as u64;
            if tick_counter >= 300 && tick_stat_start.elapsed() >= Duration::from_secs(5) {
                let stat_dur = tick_stat_start.elapsed().as_secs_f64();
                let measured_hz = (tick_counter as f64) / stat_dur;
                eprintln!(
                    "[server] ⏱️ Steady Fixed Tickrate: {:.2} Hz (target: {} Hz, dt: {:.2}ms, 300 samples in {:.2}s)",
                    measured_hz,
                    cfg.tick_rate,
                    dt * 1000.0,
                    stat_dur
                );
                tick_counter = 0;
                tick_stat_start = Instant::now();
            }

            for tick_idx in 0..ticks_to_run {
                dead_rooms.clear();

                for (name, room) in server.rooms.iter_mut() {
                    room.current_tick += 1;
                    // Rule 5: Fault-isolated simulation tick (trap doesn't crash server)
                    let packed = match room.wasm.on_tick(dt) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("[server] ⚠️ guest trap during on_tick in room '{name}': {e}");
                            continue;
                        }
                    };

                    // reference-grade: capture rolling linear memory snapshot & frame checksum in < 5μs
                    let _ = room
                        .rollback_buffer
                        .save_snapshot(room.current_tick, room.wasm.raw_instance_ref());
                    let ptr = ((packed >> 32) as u32) as usize;
                    let len = (packed as u32) as usize;

                    // Rule 4: Bounds check outbound delta size
                    if len > engine_wasm::MAX_OUTBOUND_DELTA_SIZE {
                        eprintln!(
                            "[server] ⚠️ delta buffer size {len} exceeded max limit {} in room '{name}'",
                            engine_wasm::MAX_OUTBOUND_DELTA_SIZE
                        );
                        continue;
                    }

                    let is_last_tick = tick_idx + 1 == ticks_to_run;
                    if len > 0 && is_last_tick {
                        if let Err(e) = room.wasm.read_memory_into(ptr as u32, len as u32, &mut delta_buf) {
                            eprintln!("[server] ⚠️ failed to read delta memory in room '{name}': {e}");
                            continue;
                        }
                        let delta = &delta_buf[..];

                    // SFU Spatial Audio: Synchronize authoritative 3D positions into KD-Tree
                    if delta.len() >= 4 {
                        let count = u32::from_le_bytes(delta[0..4].try_into().unwrap_or([0; 4])) as usize;

                        for i in 0..count {
                            let base = 4 + i * 72;
                            if base + 32 <= delta.len() {
                                let pid = i64::from_le_bytes(delta[base..base + 8].try_into().unwrap_or([0; 8]));
                                let px = f64::from_le_bytes(delta[base + 8..base + 16].try_into().unwrap_or([0; 8]));
                                let py = f64::from_le_bytes(delta[base + 16..base + 24].try_into().unwrap_or([0; 8]));
                                let pz = f64::from_le_bytes(delta[base + 24..base + 32].try_into().unwrap_or([0; 8]));
                                room.voice_room.update_position(pid as u32, [px, py, pz]);
                            }
                        }
                    }

                    // 1. Broadcast to active players with their player ID
                    for (session, id) in &room.sessions {
                        wrap_delta_into(&mut frame_buf, *id, delta);
                        let sent_webrtc = if let Some(ref mut srv) = webrtc_server {
                            srv.send_unreliable(session, &frame_buf)
                        } else {
                            false
                        };
                        if !sent_webrtc {
                            let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame_buf);
                        }
                    }

                    // Flush outbound WebRTC UDP packets
                    if let Some(ref mut srv) = webrtc_server {
                        if let Some(ref sock) = webrtc_udp {
                            for (dest, data) in srv.poll_transmits() {
                                let _ = sock.send_to(&data, dest);
                            }
                        }
                    }

                    // 2. Broadcast to spectators with ID 0 (Observer mode)
                    if !room.spectators.is_empty() {
                        wrap_delta_into(&mut frame_buf, 0, delta);
                        for spec_session in &room.spectators {
                            let _ = transport.send_to(spec_session, OpCode::RoomDataBytes, &frame_buf);
                        }
                    }

                    // 3. Broadcast to UDP Unity / Unreal / Godot peers
                    if let Some(ref udp_sock) = udp_socket {
                        for (&peer_addr, &(id, _)) in &udp_peers {
                            wrap_delta_into(&mut frame_buf, id, delta);
                            let _ = udp_sock.send_to(&frame_buf, peer_addr);
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
        }

            // Cleanup inactive UDP peers
            let now = Instant::now();
            timed_out_udp.clear();
            udp_peers.retain(|&addr, (id, last_seen)| {
                if now.duration_since(*last_seen) > Duration::from_secs(10) {
                    timed_out_udp.push((addr, *id));
                    false
                } else {
                    true
                }
            });
            for (addr, id) in timed_out_udp.drain(..) {
                eprintln!("[server-udp] disconnected (timeout): {addr}");
                if id > 0 {
                    if let Some(room) = server.rooms.get_mut(&cfg.name) {
                        let _ = room.wasm.on_leave(id);
                    }
                }
            }

            for name in dead_rooms.drain(..) {
                server.rooms.remove(&name);
                eprintln!("[server] room '{name}' disposed (empty 30s)");
            }
            transport.flush().expect("flush");
        }

        // Update shared room metadata for /api/rooms HTTP discovery
        if last_meta_update.elapsed() >= Duration::from_millis(500) {
            last_meta_update = Instant::now();
            if let Ok(mut meta) = rooms_meta.lock() {
                meta.clear();
                for (name, room) in &server.rooms {
                    meta.push((name.clone(), room.sessions.len(), room.spectators.len()));
                }
            }
        }

        // ---- DEV HOT-RELOAD CHECK: watch room.ts (and room_logic.wasm) ----
        if last_wasm_check.elapsed() >= Duration::from_millis(500) {
            last_wasm_check = Instant::now();
            let mut recompilation_triggered = false;

            // 1. Detect changes in the TypeScript room source (room.ts)
            if let Ok(src_current) = std::fs::metadata(&source_path).and_then(|m| m.modified()) {
                if loaded_src_time != Some(src_current) {
                    eprintln!(
                        "[server] 📝 Detected change in {} — recompiling room logic...",
                        cfg.source.display()
                    );
                    match crate::compile::build(project_dir, cfg) {
                        Ok(_) => {
                            loaded_src_time = Some(src_current);
                            recompilation_triggered = true;
                        }
                        Err(e) => {
                            eprintln!("[server] ⚠️ Compilation error in {}: {e}", cfg.source.display());
                            // Record mtime so we don't spam compile errors every 500ms
                            loaded_src_time = Some(src_current);
                        }
                    }
                }
            }

            // 2. Detect changes in compiled wasm (either from step 1 or external compiler)
            if let Ok(current) = std::fs::metadata(wasm_path).and_then(|m| m.modified()) {
                if loaded_wasm_time != Some(current) || recompilation_triggered {
                    if let Ok(bytes) = std::fs::read(wasm_path) {
                        match runtime
                            .compile(&bytes)
                            .and_then(|module| {
                                let linker = build_host_linker(&runtime, server.permissions);
                                runtime.prepare(&linker, &module)
                            }) {
                            Ok(pre) => {
                                server.pre = pre;
                                loaded_wasm_time = Some(current);
                                eprintln!(
                                    "[server] 🔥 DEV HOT-RELOAD: room logic updated ({} bytes)!",
                                    bytes.len()
                                );

                                // 🔥 Hot-swap all actively running rooms without disconnecting players!
                                for (room_name, room) in &mut server.rooms {
                                    let physics = room.wasm.data().clone();
                                    match RoomLogic::from_pre(&server.pre, physics) {
                                        Ok(mut new_wasm) => {
                                            let _ = new_wasm.on_create(server.max_clients);
                                            // Re-join active players so state is maintained seamlessly
                                            for (_sid, &pid) in &room.sessions {
                                                let (x, y) = (
                                                    60.0 + ((pid as f64) * 97.0) % (cfg.width - 120.0),
                                                    60.0 + ((pid as f64) * 61.0) % (cfg.height - 120.0),
                                                );
                                                let _ = new_wasm.on_join(pid, x, y);
                                            }
                                            room.wasm = new_wasm;
                                            eprintln!(
                                                "[server] 🔄 Room '{room_name}' hot-reloaded with {} active players!",
                                                room.sessions.len()
                                            );
                                        }
                                        Err(e) => {
                                            eprintln!("[server] ⚠️ Failed to re-instantiate room '{room_name}': {e}");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[server] ⚠️ hot-reload compilation failed: {e}");
                            }
                        }
                    }
                }
            }
        }

        transport.flush().expect("flush");
    }
}
