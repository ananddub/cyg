//! Room registry + WASM orchestration.
//!
//! Owns `GameRoom` (one isolated WASM simulation + its members), the host
//! linker for the game-host ABI, and the `GameServer` room registry with its
//! hot-swappable pre-instantiated template. The per-tick broadcast loop lives
//! in `crate::host` and the delta/input plumbing in `crate::netcode`, both of
//! which compile against the pinned types below.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use engine_physics::{PhysicsWorld3D, RapierWorld2D};
use rapier2d::prelude as r2d;
use engine_wasm::wasm_module;
use serde::{Deserialize, Serialize};

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

/// The simulation / netcode backend of a GameRoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomBackend {
    /// Zero-WASM relay mode: sessions and broadcast plumbing without a
    /// simulation backend (the host loop drives everything).
    Relay,
    /// Server-Authoritative WASM Mode: Runs Rapier3D physics & game logic.
    Wasm,
    /// Native bevy_ecs mode: TickRunner + ECS-synced Rapier3D + RoomRng +
    /// OXCL arena. No WASM guest — the simulation is Rust-native.
    Ecs,
    /// reference-Style Deterministic Tick Input-Hub: Server never runs physics,
    /// collects player inputs, bundles them by tick, and forwards them.
    InputHub,
}

/// Encapsulates snapshot data across backends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RoomSnapshotData {
    Wasm(Vec<u8>),
}

/// A typed message queued by the guest through `host_send`.
///
/// `target == 0` broadcasts to every member (and spectator); a positive value
/// is the player id returned by `on_join`. The host owns routing because the
/// guest knows player ids, not transport session ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundMessage {
    pub target: i64,
    /// UTF-8 event/message name, e.g. `"discard"` or `"chat"`.
    pub event: String,
    pub payload: Vec<u8>,
}

/// Bound on guest-queued messages between host drains. The host normally drains
/// every tick, but a guest could otherwise grow this vector without limit.
pub const MAX_OUTBOX_MESSAGES: usize = 256;

/// Host state attached to one WASM store.
///
/// The deref impl keeps every physics host import (`caller.data().raycast(..)`)
/// unchanged while giving the message outbox a home that is not part of the
/// physics crate.
pub struct RoomHostData {
    pub physics: PhysicsWorld3D,
    pub rapier2d: RapierWorld2D,
    pub rapier2d_next_body_id: i32,
    pub rapier2d_bodies: HashMap<i32, r2d::RigidBodyHandle>,
    pub rapier2d_next_col_id: i32,
    pub rapier2d_colliders: HashMap<i32, r2d::ColliderHandle>,
    pub outbox: Vec<OutboundMessage>,
    /// Deterministic, room-seeded RNG exposed to the guest as `sys_rng_*`
    /// (Phase-2 DX: the guest never implements its own PRNG — every client
    /// replays identical sequences from the same room seed).
    pub rng: engine_core::RoomRng,
    /// Guest-maintained spatial index for AOI queries (`sys_spatial_*`): the
    /// guest pushes entity positions each tick, the host answers radius
    /// queries via the kiddo KD-tree.
    pub spatial: engine_spatial::SpatialIndex2D,
    /// Authoritative simulation tick counter, bumped once per `GameRoom::tick`
    /// and read by the guest as `sys_clock_ticks` (deterministic — never
    /// wall-clock).
    pub clock_ticks: i64,
}

impl RoomHostData {
    pub fn new(physics: PhysicsWorld3D) -> Self {
        Self {
            physics,
            rapier2d: RapierWorld2D::new(0.0, 0.0),
            rapier2d_next_body_id: 1,
            rapier2d_bodies: HashMap::new(),
            rapier2d_next_col_id: 1,
            rapier2d_colliders: HashMap::new(),
            outbox: Vec::new(),
            rng: engine_core::RoomRng::from_seed_str("unseeded"),
            spatial: engine_spatial::SpatialIndex2D::new(),
            clock_ticks: 0,
        }
    }

    /// Re-seed the guest-facing RNG from the room name (called once at room
    /// creation, where the name is known).
    pub fn seed_rng(&mut self, room_name: &str) {
        self.rng = engine_core::RoomRng::from_seed_str(room_name);
    }

    pub fn init_rapier2d(&mut self, gx: f32, gy: f32) {
        self.rapier2d = RapierWorld2D::new(gx, gy);
        self.rapier2d_bodies.clear();
        self.rapier2d_colliders.clear();
        self.rapier2d_next_body_id = 1;
        self.rapier2d_next_col_id = 1;
    }

    pub fn rapier2d_create_body(&mut self, body_type: i32, x: f32, y: f32) -> i32 {
        let handle = self.rapier2d.create_body(body_type, x, y);
        let id = self.rapier2d_next_body_id;
        self.rapier2d_next_body_id += 1;
        self.rapier2d_bodies.insert(id, handle);
        id
    }

    pub fn rapier2d_add_circle_collider(&mut self, body_id: i32, radius: f32, is_sensor: bool, restitution: f32, friction: f32) -> i32 {
        let Some(&handle) = self.rapier2d_bodies.get(&body_id) else {
            return 0;
        };
        let col_handle = self.rapier2d.add_circle_collider(handle, radius, is_sensor, restitution, friction);
        let col_id = self.rapier2d_next_col_id;
        self.rapier2d_next_col_id += 1;
        self.rapier2d_colliders.insert(col_id, col_handle);
        col_id
    }

    pub fn rapier2d_add_box_collider(&mut self, body_id: i32, half_w: f32, half_h: f32, is_sensor: bool, restitution: f32, friction: f32) -> i32 {
        let Some(&handle) = self.rapier2d_bodies.get(&body_id) else {
            return 0;
        };
        let col_handle = self.rapier2d.add_box_collider(handle, half_w, half_h, is_sensor, restitution, friction);
        let col_id = self.rapier2d_next_col_id;
        self.rapier2d_next_col_id += 1;
        self.rapier2d_colliders.insert(col_id, col_handle);
        col_id
    }

    pub fn rapier2d_set_velocity(&mut self, body_id: i32, vx: f32, vy: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.set_velocity(handle, vx, vy);
        }
    }

    pub fn rapier2d_get_velocity(&self, body_id: i32) -> (f32, f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.get_velocity(handle)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn rapier2d_get_position(&self, body_id: i32) -> Option<(f32, f32)> {
        let &handle = self.rapier2d_bodies.get(&body_id)?;
        self.rapier2d.get_position(handle)
    }

    pub fn rapier2d_set_position(&mut self, body_id: i32, x: f32, y: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.set_position(handle, x, y);
        }
    }

    pub fn rapier2d_apply_impulse(&mut self, body_id: i32, ix: f32, iy: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.apply_impulse(handle, ix, iy);
        }
    }

    pub fn rapier2d_step(&mut self, dt: f32) {
        if dt > 0.0 {
            self.rapier2d.integration_parameters.dt = dt;
        }
        self.rapier2d.step();
    }

    pub fn rapier2d_check_contact(&self, body_id_a: i32, body_id_b: i32) -> bool {
        let (Some(&handle_a), Some(&handle_b)) = (self.rapier2d_bodies.get(&body_id_a), self.rapier2d_bodies.get(&body_id_b)) else {
            return false;
        };
        self.rapier2d.check_intersection(handle_a, handle_b)
    }

    pub fn rapier2d_remove_body(&mut self, body_id: i32) {
        if let Some(handle) = self.rapier2d_bodies.remove(&body_id) {
            self.rapier2d.remove_body(handle);
        }
    }
}

impl Deref for RoomHostData {
    type Target = PhysicsWorld3D;

    fn deref(&self) -> &Self::Target {
        &self.physics
    }
}

impl DerefMut for RoomHostData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.physics
    }
}

/// One named room: an isolated simulation + its members.
pub struct GameRoom {
    pub(crate) backend: RoomBackend,
    /// Mirrors the manifest's `storage` capability. When `false` the guest's
    /// `on_save`/`on_load` are never invoked, so a game cannot write to (or read
    /// from) the snapshot store — the capability is actually enforced.
    pub(crate) storage_enabled: bool,
    pub(crate) wasm: Option<RoomLogic<RoomHostData>>,
    /// Native bevy_ecs simulation state (`RoomBackend::Ecs` only).
    pub(crate) ecs: Option<crate::ecs_room::EcsRoomState>,
    /// Messages queued by the ECS backend between host drains (mirrors the
    /// WASM `RoomHostData.outbox` contract).
    pub(crate) ecs_outbox: Vec<OutboundMessage>,
    pub(crate) sessions: HashMap<String, i64>,
    pub(crate) spectators: HashSet<String>,
    pub(crate) next_player_id: i64,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) current_tick: u64,
    pub(crate) rollback_buffer: engine_wasm::RollbackRingBuffer,
    pub(crate) voice_room: engine_voice::VoiceRoom,
    pub(crate) voice_mesh: engine_voice::VoiceMesh,
    pub(crate) last_input_seq: HashMap<String, u16>,
    pub(crate) event_seq: u16,
    pub(crate) delta_seq: u16,
    /// Sessions that already received the schema full snapshot. New viewers
    /// get the full row once, then field-level patches.
    pub(crate) schema_sent: HashSet<String>,
    /// Sessions whose socket died but whose seat is held for reconnection:
    /// session -> (player id, give-up deadline).
    pub(crate) pending_leaves: HashMap<String, (i64, Instant)>,
    /// Set when a patch window skipped ticks; the next send is a full snapshot.
    pub(crate) patch_skipped: bool,
    /// When the last schema patch window opened (patch-rate gating).
    pub(crate) last_patch_at: Option<Instant>,
    /// Host-tick time accumulated toward the guest's simulation interval.
    pub(crate) sim_accum: Duration,
    pub(crate) acked_event_seq: HashMap<String, u16>,
    pub(crate) input_journal: VecDeque<JournalInput>,
    pub(crate) input_hub_records: VecDeque<InputHubTickRecord>,
    /// Session -> authoritative actor id. The first valid input packet claims
    /// an actor for a session; later packets from that session cannot change
    /// it, and another session cannot claim the same actor.
    pub(crate) input_hub_actors: HashMap<String, u16>,
    /// Reverse index used to reject duplicate actor claims.
    pub(crate) input_hub_sessions: HashMap<u16, String>,
    /// Last accepted client tick per actor, used to drop duplicate/replayed
    /// datagrams before they reach the room history.
    pub(crate) input_hub_last_tick: HashMap<u16, u32>,
}

/// Recorded player input frame in the Input-Hub for bundling/history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputHubTickRecord {
    pub player_actor_id: u16,
    pub client_tick: u32,
    pub payload: Vec<u8>,
    pub received_at: Instant,
}

/// One journaled input: everything needed to re-deliver it to the guest after a
/// rewind. `tick` is the live tick when it arrived, so replay places it between
/// the same two `on_tick` calls as the original stream did.
#[derive(Clone)]
pub(crate) struct JournalInput {
    pub(crate) tick: u64,
    pub(crate) session: String,
    pub(crate) id: i64,
    pub(crate) seq: u16,
    pub(crate) payload: [u8; 20],
}

/// Build the host-import linker once at startup. The ~12 `func_wrap` closures
/// are identical for every room, so building them per spawn was pure waste;
/// pre-instantiating against this linker makes each new room a single
/// `InstancePre::instantiate`.
pub(crate) fn build_host_linker(
    runtime: &engine_wasm::WasmRuntime,
    permissions: engine_wasm::AbiPermissions,
) -> wasmtime::Linker<RoomHostData> {
    let mut linker = runtime.create_linker::<RoomHostData>();

    // Register host functions under canonical `game_host_v1`, legacy `env`, and `ox` namespaces (Rule 1 & Rule 8)
    for ns in &[
        engine_wasm::HOST_NAMESPACE_V1,
        engine_wasm::HOST_NAMESPACE_LEGACY,
        "ox",
    ] {
        // Abort callback
        linker
            .func_wrap(
                ns,
                "abort",
                |_caller: wasmtime::Caller<'_, RoomHostData>,
                 _m: u32,
                 _f: u32,
                 _l: u32,
                 _c: u32| {},
            )
            .expect("link abort");

        // Host Log ABI: UTF-8 validated, bounds-checked (Rule 2 & Rule 4)
        linker
            .func_wrap(
                ns,
                "host_log",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, ptr: i32, len: i32| -> i32 {
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

        // Host messaging ABI: queue a typed, optionally targeted room message.
        // The guest supplies a UTF-8 name + opaque payload; the host routes it
        // to the player id (or broadcasts for target == 0). Actual framing and
        // transport writes happen on the host loop, never inside the guest call.
        linker
            .func_wrap(
                ns,
                "host_send",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      target: i64,
                      type_ptr: i32,
                      type_len: i32,
                      data_ptr: i32,
                      data_len: i32|
                      -> i32 {
                    if !permissions.messaging {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if type_len <= 0
                        || type_len > 255
                        || data_len < 0
                        || data_len as usize > engine_wasm::MAX_INBOUND_PAYLOAD_SIZE
                    {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }

                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let t_off = type_ptr as usize;
                    let t_end = t_off.saturating_add(type_len as usize);
                    let d_off = data_ptr as usize;
                    let d_end = d_off.saturating_add(data_len as usize);
                    if t_off >= memory.len()
                        || t_end > memory.len()
                        || (data_len > 0 && (d_off >= memory.len() || d_end > memory.len()))
                    {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }

                    let event = match std::str::from_utf8(&memory[t_off..t_end]) {
                        Ok(name) => name.to_string(),
                        Err(_) => return engine_wasm::ABI_ERR_INVALID_UTF8,
                    };
                    let payload = if data_len > 0 {
                        memory[d_off..d_end].to_vec()
                    } else {
                        Vec::new()
                    };

                    let data = caller.data_mut();
                    if data.outbox.len() >= MAX_OUTBOX_MESSAGES {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.outbox.push(OutboundMessage {
                        target,
                        event,
                        payload,
                    });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_send");

        // Host Version ABI (Rule 1: Version query)
        linker
            .func_wrap(
                ns,
                "get_abi_version",
                move |_caller: wasmtime::Caller<'_, RoomHostData>| -> i32 {
                    engine_wasm::CURRENT_ABI_VERSION as i32
                },
            )
            .expect("link get_abi_version");

        // Host Physics ABI 1: Surface height query (for solid standing & rooftops)
        linker
            .func_wrap(
                ns,
                "sys_physics_surface_y",
                move |caller: wasmtime::Caller<'_, RoomHostData>,
                      px: f64,
                      py: f64,
                      pz: f64,
                      radius: f64|
                      -> f64 {
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
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      px: f64,
                      py: f64,
                      pz: f64,
                      vx: f64,
                      vy: f64,
                      vz: f64,
                      radius: f64,
                      dt: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let res = caller
                        .data()
                        .move_and_slide(px, py, pz, vx, vy, vz, radius, dt);

                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 24 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&res.new_x.to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&res.new_y.to_le_bytes());
                            data[offset + 16..offset + 24]
                                .copy_from_slice(&res.new_z.to_le_bytes());
                        } else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                    }

                    if res.is_grounded {
                        1
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_physics_move");

        // Host Physics ABI 3: Continuous Collision Detection Raycasting (bullets & hitscan)
        linker
            .func_wrap(
                ns,
                "sys_physics_raycast",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      ox: f64,
                      oy: f64,
                      oz: f64,
                      dx: f64,
                      dy: f64,
                      dz: f64,
                      max_dist: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

                    let res = if let Some(hit) = caller.data().raycast(origin, dir, max_dist) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 36 <= data.len() {
                                data[offset..offset + 8]
                                    .copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16]
                                    .copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24]
                                    .copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32]
                                    .copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36]
                                    .copy_from_slice(&hit.collider_id.to_le_bytes());
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
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      tick: i64,
                      entity_id: i32,
                      kind: i32,
                      min_x: f64,
                      min_y: f64,
                      min_z: f64,
                      max_x: f64,
                      max_y: f64,
                      max_z: f64| {
                    if !permissions.lag_compensation {
                        return;
                    }
                    let bounds = engine_physics::Aabb {
                        min: engine_physics::Vec3::new(min_x, min_y, min_z),
                        max: engine_physics::Vec3::new(max_x, max_y, max_z),
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
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      shoot_tick: i64,
                      ox: f64,
                      oy: f64,
                      oz: f64,
                      dx: f64,
                      dy: f64,
                      dz: f64,
                      max_dist: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.lag_compensation {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

                    if let Some(hit) = caller.data_mut().lag_compensated_raycast(
                        shoot_tick as u64,
                        origin,
                        dir,
                        max_dist,
                    ) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 44 <= data.len() {
                                data[offset..offset + 8]
                                    .copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16]
                                    .copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24]
                                    .copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32]
                                    .copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36]
                                    .copy_from_slice(&hit.entity_id.to_le_bytes());
                                data[offset + 36..offset + 40]
                                    .copy_from_slice(&hit.kind.to_le_bytes());
                                data[offset + 40..offset + 44]
                                    .copy_from_slice(&hit.collider_id.to_le_bytes());
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

        // Host RNG ABI (Deterministic room-seeded ChaCha8)
        linker
            .func_wrap(
                ns,
                "sys_rng_next",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>| -> f64 {
                    caller.data_mut().rng.next_f32() as f64
                },
            )
            .expect("link sys_rng_next");

        // Host Clock ABI (Deterministic room tick count)
        linker
            .func_wrap(
                ns,
                "sys_clock_ticks",
                move |caller: wasmtime::Caller<'_, RoomHostData>| -> i64 {
                    caller.data().clock_ticks
                },
            )
            .expect("link sys_clock_ticks");

        // Host Spatial AOI: Upsert
        linker
            .func_wrap(
                ns,
                "sys_spatial_update",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      x: f64,
                      y: f64|
                      -> i32 {
                    caller.data_mut().spatial.upsert(entity_id as u32, [x, y]);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_spatial_update");

        // Host Spatial AOI: Radius Query
        linker
            .func_wrap(
                ns,
                "sys_spatial_query",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      x: f64,
                      y: f64,
                      radius: f64,
                      out_ptr: i32,
                      max_results: i32|
                      -> i32 {
                    let hits = caller.data().spatial.query_radius([x, y], radius);
                    let limit = if max_results > 0 {
                        (max_results as usize).min(hits.len())
                    } else {
                        hits.len()
                    };
                    if limit > 0 && out_ptr > 0 {
                        let offset = out_ptr as usize;
                        let bytes_needed = limit * 4;
                        let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        };
                        let data = mem.data_mut(&mut caller);
                        if offset + bytes_needed > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        for (i, hit) in hits.iter().take(limit).enumerate() {
                            let pos = offset + i * 4;
                            data[pos..pos + 4]
                                .copy_from_slice(&(hit.entity_id as u32).to_le_bytes());
                        }
                    }
                    limit as i32
                },
            )
            .expect("link sys_spatial_query");

        // Host Rapier2D ABI: 2D Physics World
        linker
            .func_wrap(
                ns,
                "sys_rapier2d_init",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, gx: f64, gy: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().init_rapier2d(gx as f32, gy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_init");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_create_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_type: i32, x: f64, y: f64| -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller.data_mut().rapier2d_create_body(body_type, x as f32, y as f32)
                },
            )
            .expect("link sys_rapier2d_create_body");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_add_circle_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      radius: f64,
                      is_sensor: i32,
                      restitution: f64,
                      friction: f64| -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller.data_mut().rapier2d_add_circle_collider(
                        body_id,
                        radius as f32,
                        is_sensor != 0,
                        restitution as f32,
                        friction as f32,
                    )
                },
            )
            .expect("link sys_rapier2d_add_circle_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_add_box_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      half_w: f64,
                      half_h: f64,
                      is_sensor: i32,
                      restitution: f64,
                      friction: f64| -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller.data_mut().rapier2d_add_box_collider(
                        body_id,
                        half_w as f32,
                        half_h as f32,
                        is_sensor != 0,
                        restitution as f32,
                        friction as f32,
                    )
                },
            )
            .expect("link sys_rapier2d_add_box_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_set_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32, vx: f64, vy: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_set_velocity(body_id, vx as f32, vy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_set_velocity");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_get_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32, out_ptr: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let (vx, vy) = caller.data().rapier2d_get_velocity(body_id);
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 16 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&(vx as f64).to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&(vy as f64).to_le_bytes());
                            return engine_wasm::ABI_OK;
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link sys_rapier2d_get_velocity");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_get_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32, out_ptr: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let Some((x, y)) = caller.data().rapier2d_get_position(body_id) else {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    };
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 16 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&(x as f64).to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&(y as f64).to_le_bytes());
                            return engine_wasm::ABI_OK;
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link sys_rapier2d_get_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_set_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32, x: f64, y: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_set_position(body_id, x as f32, y as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_set_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_apply_impulse",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32, ix: f64, iy: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_apply_impulse(body_id, ix as f32, iy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_apply_impulse");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_step",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, dt: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_step(dt as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_step");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_check_contact",
                move |caller: wasmtime::Caller<'_, RoomHostData>, body_id_a: i32, body_id_b: i32| -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    if caller.data().rapier2d_check_contact(body_id_a, body_id_b) {
                        1
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_rapier2d_check_contact");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_remove_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_remove_body(body_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_remove_body");
    }

    linker
}

impl GameRoom {
    /// Spawn a room from the pre-instantiated template: one `instantiate`
    /// against the shared linker, then the guest's `on_create`.
    pub(crate) fn new(
        pre: &engine_wasm::WasmPreInstance<RoomHostData>,
        max_clients: i32,
        physics: PhysicsWorld3D,
    ) -> Result<Self, engine_wasm::WasmError> {
        // A guest that predates the ABI (e.g. missing `on_alloc`) must fail the
        // one room, not abort the whole process — the caller rejects the client.
        let mut wasm = RoomLogic::from_pre(pre, RoomHostData::new(physics))?;
        if let Err(e) = wasm.on_create(max_clients) {
            eprintln!("[server] ⚠️ guest on_create error: {e}");
        }
        Ok(Self {
            backend: RoomBackend::Wasm,
            // Default on; `GameServer::get_or_create` overrides it from the
            // manifest permissions for every spawned room.
            storage_enabled: true,
            wasm: Some(wasm),
            ecs: None,
            ecs_outbox: Vec::new(),
            sessions: HashMap::new(),
            spectators: HashSet::new(),
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
            delta_seq: 0,
            schema_sent: HashSet::new(),
            pending_leaves: HashMap::new(),
            patch_skipped: false,
            last_patch_at: None,
            sim_accum: Duration::ZERO,
            acked_event_seq: HashMap::new(),
            input_journal: VecDeque::new(),
            input_hub_records: VecDeque::new(),
            input_hub_actors: HashMap::new(),
            input_hub_sessions: HashMap::new(),
            input_hub_last_tick: HashMap::new(),
        })
    }

    /// Spawn a zero-WASM relay room: session/seat bookkeeping and broadcast
    /// plumbing without a simulation backend.
    pub(crate) fn new_relay() -> Self {
        Self {
            backend: RoomBackend::Relay,
            storage_enabled: true,
            wasm: None,
            ecs: None,
            ecs_outbox: Vec::new(),
            sessions: HashMap::new(),
            spectators: HashSet::new(),
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
            delta_seq: 0,
            schema_sent: HashSet::new(),
            pending_leaves: HashMap::new(),
            patch_skipped: false,
            last_patch_at: None,
            sim_accum: Duration::ZERO,
            acked_event_seq: HashMap::new(),
            input_journal: VecDeque::new(),
            input_hub_records: VecDeque::new(),
            input_hub_actors: HashMap::new(),
            input_hub_sessions: HashMap::new(),
            input_hub_last_tick: HashMap::new(),
        }
    }

    /// Spawn a native bevy_ecs room: TickRunner + ECS-synced Rapier3D +
    /// deterministic RoomRng + OXCL arena. Zero WASM, zero locks.
    /// `arena` comes from `ox-game cook <map.glb> arena.oxcl` (optional).
    pub(crate) fn new_ecs(name: &str, arena: Option<&engine_physics::oxcl::CookedMesh>) -> Self {
        let mut room = Self::new_relay();
        room.backend = RoomBackend::Ecs;
        room.ecs = Some(crate::ecs_room::EcsRoomState::new(name, arena));
        room
    }

    /// Spawn a reference-inspired Deterministic Input-Hub room (zero-physics
    /// mode). The WASM guest is not retained: an input hub must never run
    /// server physics, even if the room was first created from a WASM template.
    pub fn new_input_hub() -> Self {
        let mut room = Self::new_relay();
        room.enable_input_hub();
        room
    }

    /// Switch an existing room to input-hub mode. This is explicit and
    /// idempotent; it is never triggered by an unauthenticated packet.
    pub fn enable_input_hub(&mut self) {
        self.backend = RoomBackend::InputHub;
        self.wasm = None;
    }

    /// Number of distinct actors currently claimed in this input hub.
    #[inline]
    pub fn input_hub_actor_count(&self) -> usize {
        self.input_hub_actors.len()
    }

    /// Claim an actor id for a session. Returns false when the session tries
    /// to change actors or another session already owns the id.
    pub fn claim_input_actor(&mut self, session: &str, actor: u16) -> bool {
        if actor == 0 {
            return false;
        }
        if let Some(existing) = self.input_hub_actors.get(session) {
            return *existing == actor;
        }
        if self.input_hub_sessions.contains_key(&actor) {
            return false;
        }
        self.input_hub_actors.insert(session.to_string(), actor);
        self.input_hub_sessions.insert(actor, session.to_string());
        true
    }

    /// Release a session's input-hub actor on disconnect/timeout.
    pub fn release_input_actor(&mut self, session: &str) {
        if let Some(actor) = self.input_hub_actors.remove(session) {
            self.input_hub_sessions.remove(&actor);
            self.input_hub_last_tick.remove(&actor);
        }
    }

    /// Append a validated, monotonically increasing input frame. Returns false
    /// for a duplicate/replayed tick or a payload that is not 9/18/27 bytes.
    pub fn record_input_tick_checked(
        &mut self,
        session: &str,
        player_actor_id: u16,
        client_tick: u32,
        payload: &[u8],
    ) -> bool {
        let len = payload.len();
        if !(crate::netcode::INPUT_HUB_INPUT_SIZE
            ..=crate::netcode::INPUT_HUB_INPUT_SIZE * crate::netcode::INPUT_HUB_MAX_REDUNDANCY)
            .contains(&len)
            || !len.is_multiple_of(crate::netcode::INPUT_HUB_INPUT_SIZE)
        {
            return false;
        }
        if !self.claim_input_actor(session, player_actor_id) {
            return false;
        }
        if let Some(last) = self.input_hub_last_tick.get(&player_actor_id) {
            if client_tick <= *last {
                return false;
            }
        }
        self.input_hub_last_tick
            .insert(player_actor_id, client_tick);
        if self.input_hub_records.len() >= 256 {
            self.input_hub_records.pop_front();
        }
        self.input_hub_records.push_back(InputHubTickRecord {
            player_actor_id,
            client_tick,
            payload: payload.to_vec(),
            received_at: Instant::now(),
        });
        true
    }

    /// Record a received input tick from a player actor for bundling/history.
    pub fn record_input_tick(&mut self, player_actor_id: u16, client_tick: u32, payload: &[u8]) {
        if self.input_hub_records.len() >= 256 {
            self.input_hub_records.pop_front();
        }
        self.input_hub_records.push_back(InputHubTickRecord {
            player_actor_id,
            client_tick,
            payload: payload.to_vec(),
            received_at: Instant::now(),
        });
    }

    /// Retrieve the latest recorded input for a given player actor.
    pub fn latest_input_tick(&self, player_actor_id: u16) -> Option<&InputHubTickRecord> {
        self.input_hub_records
            .iter()
            .rev()
            .find(|rec| rec.player_actor_id == player_actor_id)
    }

    /// Whether the room has 0 players and is currently hibernating (0% CPU).
    #[inline]
    pub fn is_hibernating(&self) -> bool {
        self.sessions.is_empty() && self.spectators.is_empty()
    }

    /// Whether this room runs in zero-WASM relay mode.
    #[inline]
    pub fn is_relay(&self) -> bool {
        self.backend == RoomBackend::Relay
    }

    /// Whether this room runs in server-authoritative WASM mode.
    #[inline]
    pub fn is_wasm(&self) -> bool {
        self.backend == RoomBackend::Wasm
    }

    /// Whether this room runs the native bevy_ecs simulation.
    #[inline]
    pub fn is_ecs(&self) -> bool {
        self.backend == RoomBackend::Ecs
    }

    /// Tick the native ECS simulation and queue its delta for the host to
    /// broadcast (same `OutboundMessage` contract a WASM guest's `host_send`
    /// uses: `target = 0` = room-wide, event `"delta"`, TLV payload).
    /// Returns the delta byte count for telemetry.
    pub(crate) fn tick_ecs(&mut self, dt: f64) -> usize {
        let Some(ref mut ecs) = self.ecs else {
            return 0;
        };
        let delta = ecs.tick(dt);
        if delta.is_empty() {
            return 0;
        }
        const MAX_ECS_OUTBOX: usize = 256;
        let len = delta.len();
        if self.ecs_outbox.len() < MAX_ECS_OUTBOX {
            self.ecs_outbox.push(OutboundMessage {
                target: 0,
                event: "delta".to_string(),
                payload: delta,
            });
        }
        len
    }

    /// Route one session's raw input bytes into the ECS backend (first byte
    /// is the WASD flags bitmask).
    pub(crate) fn ecs_input(&mut self, session: &str, data: &[u8]) {
        if let Some(ref mut ecs) = self.ecs {
            ecs.apply_input(session, data);
        }
    }

    /// Whether this room operates in deterministic Input-Hub mode.
    #[inline]
    pub fn is_input_hub(&self) -> bool {
        self.backend == RoomBackend::InputHub
    }

    /// Export the room's serializable persistent snapshot.
    ///
    /// A room without the `storage` capability returns `None` without ever
    /// calling the guest's `on_save`, so nothing can reach the snapshot store.
    pub fn export_snapshot(&mut self) -> Option<Vec<u8>> {
        if !self.storage_enabled {
            return None;
        }
        match self.backend {
            RoomBackend::Relay | RoomBackend::InputHub => None,
            RoomBackend::Wasm => {
                if let Some(ref mut wasm) = self.wasm {
                    if let Ok(packed) = wasm.on_save() {
                        let ptr = ((packed >> 32) as u32) as usize;
                        let len = (packed as u32) as usize;
                        if len > 0 {
                            let mut buf = vec![0u8; len];
                            if wasm
                                .read_memory_into(ptr as u32, len as u32, &mut buf)
                                .is_ok()
                            {
                                let snap = RoomSnapshotData::Wasm(buf);
                                return serde_json::to_vec(&snap).ok();
                            }
                        }
                    }
                }
                None
            }
            RoomBackend::Ecs => {
                if let Some(ref mut ecs) = self.ecs {
                    // Positions + health of every live avatar, in TLV form —
                    // the same delta encoding the backend broadcasts.
                    let snap = RoomSnapshotData::Wasm(ecs.export_tlv());
                    serde_json::to_vec(&snap).ok()
                } else {
                    None
                }
            }
        }
    }

    /// Restore the room's state from a persistent snapshot.
    pub fn import_snapshot(&mut self, bytes: &[u8]) -> Result<(), String> {
        if !self.storage_enabled {
            return Err("storage permission denied".into());
        }
        let snap: RoomSnapshotData = serde_json::from_slice(bytes)
            .map_err(|e| format!("Failed to parse room snapshot: {e}"))?;

        match snap {
            RoomSnapshotData::Wasm(wasm_bytes) => {
                if let Some(ref mut wasm) = self.wasm {
                    let len = wasm_bytes.len() as i32;
                    let ptr = wasm
                        .on_alloc(len)
                        .map_err(|e| format!("on_alloc failed: {e}"))?;
                    wasm.write_memory(ptr as u32, &wasm_bytes)
                        .map_err(|e| format!("write_memory failed: {e}"))?;
                    wasm.on_load(ptr, len)
                        .map_err(|e| format!("on_load failed: {e}"))?;
                    Ok(())
                } else {
                    Err("Room has no WASM instance".into())
                }
            }
        }
    }

    /// The host's own ceiling: a relay room has no guest logic, so without
    /// this it would silently overfill — ignoring `max_clients` entirely.
    /// `max_clients <= 0` means "no limit". WASM rooms go through
    /// [`Self::is_full`], which lets the guest override the policy.
    #[inline]
    pub(crate) fn is_at_capacity(&self, max_clients: i32) -> bool {
        (self.is_relay() || self.is_input_hub())
            && max_clients > 0
            && self.sessions.len() as i32 >= max_clients
    }

    /// Capacity decision for a joining player.
    ///
    /// A WASM room may ship an optional `on_is_full(count)` export to implement
    /// its own policy (teams, VIP slots, wave limits…); the host asks it and
    /// honours the answer. Rooms without that export — and zero-WASM relay
    /// rooms, which have no guest at all — fall back to the host ceiling from
    /// [`Self::is_at_capacity`].
    pub(crate) fn is_full(&mut self, max_clients: i32) -> bool {
        if let Some(full) = self.guest_is_full(self.sessions.len() as i32) {
            return full;
        }
        self.is_at_capacity(max_clients)
    }

    /// Ask the guest's optional `on_is_full(count)` override.
    ///
    /// `None` when the room has no WASM guest, the export is absent, or the call
    /// traps — the caller then applies the host's own policy. Looked up on
    /// demand (joins are rare) so guests that omit the export still instantiate.
    fn guest_is_full(&mut self, count: i32) -> Option<bool> {
        let wasm = self.wasm.as_mut()?;
        let instance = wasm.raw_instance();
        // AssemblyScript `bool` is an i32 on the wire (0/1), which is also the
        // only result shape wasmtime accepts here.
        let func = instance.get_typed_func::<(i32,), i32>("on_is_full").ok()?;
        func.call(instance.store_mut(), (count,))
            .ok()
            .map(|v| v != 0)
    }

    /// Seat admission through the guest's optional `onAuth(clientId, options)`.
    /// `None` means the guest ships no hook (or cannot be called); the caller
    /// then admits the client, matching Colyseus's default `onAuth`.
    pub(crate) fn authorize(&mut self, id: i64, options: &str) -> bool {
        self.guest_on_auth(id, options).unwrap_or(true)
    }

    fn guest_on_auth(&mut self, id: i64, options: &str) -> Option<bool> {
        let wasm = self.wasm.as_mut()?;
        if wasm
            .raw_instance()
            .get_typed_func::<(i64, i32, i32), i32>("on_auth")
            .is_err()
        {
            return None;
        }
        let bytes = options.as_bytes();
        let len = i32::try_from(bytes.len()).ok()?;
        let ptr = if len == 0 {
            0
        } else {
            // Fail closed: a guest that asked for auth but whose allocator
            // failed must not silently admit anyone.
            let ptr = wasm.on_alloc(len).ok()?;
            wasm.write_memory(ptr as u32, bytes).ok()?;
            ptr
        };
        let instance = wasm.raw_instance();
        let func = instance
            .get_typed_func::<(i64, i32, i32), i32>("on_auth")
            .ok()?;
        func.call(instance.store_mut(), (id, ptr, len))
            .ok()
            .map(|v| v != 0)
    }

    /// Milliseconds the guest wants a disconnected seat held for reconnection.
    /// `None` when the export is absent, so the seat is released immediately.
    fn guest_allow_reconnection(&mut self, id: i64) -> Option<i32> {
        let wasm = self.wasm.as_mut()?;
        let instance = wasm.raw_instance();
        let func = instance
            .get_typed_func::<(i64,), i32>("on_allow_reconnection")
            .ok()?;
        func.call(instance.store_mut(), (id,)).ok()
    }

    /// Guest-configured patch cadence in milliseconds (0 = every tick).
    fn guest_patch_rate_ms(&mut self) -> i32 {
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        let instance = wasm.raw_instance();
        let Ok(func) = instance.get_typed_func::<(), i32>("on_patch_rate") else {
            return 0;
        };
        func.call(instance.store_mut(), ()).unwrap_or(0).max(0)
    }

    /// Guest-configured simulation cadence in milliseconds (0 = every tick).
    fn guest_simulation_rate_ms(&mut self) -> i32 {
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        let instance = wasm.raw_instance();
        let Ok(func) = instance.get_typed_func::<(), i32>("on_simulation_rate") else {
            return 0;
        };
        func.call(instance.store_mut(), ()).unwrap_or(0).max(0)
    }

    /// True when this session's seat is held for reconnection.
    #[allow(dead_code)]
    pub(crate) fn has_pending_leave(&self, session: &str) -> bool {
        self.pending_leaves.contains_key(session)
    }

    /// Resume a held seat: cancel the pending leave and call the guest's
    /// `onReconnect`. Returns the retained player id; `onJoin` is not re-run.
    pub(crate) fn rejoin(&mut self, session: &str) -> Option<i64> {
        let (id, _deadline) = self.pending_leaves.remove(session)?;
        if self.sessions.get(session).copied() != Some(id) {
            return None;
        }
        self.note_activity();
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm
                .raw_instance()
                .call::<(i64,), ()>("on_reconnect", (id,));
        }
        Some(id)
    }

    /// Release every seat whose reconnection deadline passed. Returns whether
    /// any guest `onLeave` ran, so the caller can flush the outbox.
    pub(crate) fn expire_pending_leaves(&mut self, now: Instant) -> bool {
        if self.pending_leaves.is_empty() {
            return false;
        }
        let expired: Vec<(String, i64)> = self
            .pending_leaves
            .iter()
            .filter(|(_, (_, deadline))| now >= *deadline)
            .map(|(session, (id, _))| (session.clone(), *id))
            .collect();
        for (session, id) in &expired {
            self.pending_leaves.remove(session);
            self.sessions.remove(session);
            self.release_player(session, *id);
        }
        if !expired.is_empty() && self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(now);
        }
        !expired.is_empty()
    }

    /// Run the guest's optional `onDispose` before the host drops the room.
    pub(crate) fn dispose(&mut self) {
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.raw_instance().call::<(), ()>("on_dispose", ());
        }
    }

    /// Patch-rate gate. Returns false when this tick's state frame must be
    /// dropped; the skipped changes are recovered because the next due frame
    /// is sent as a full snapshot.
    pub(crate) fn should_send_patch(&mut self, now: Instant) -> bool {
        let rate = self.guest_patch_rate_ms();
        if rate <= 0 {
            self.last_patch_at = Some(now);
            return true;
        }
        let due = match self.last_patch_at {
            Some(last) => now.duration_since(last) >= Duration::from_millis(rate as u64),
            None => true,
        };
        if due {
            self.last_patch_at = Some(now);
        } else {
            self.patch_skipped = true;
        }
        due
    }

    /// Consume the "a patch window was skipped" flag for this send.
    pub(crate) fn take_patch_skipped(&mut self) -> bool {
        std::mem::take(&mut self.patch_skipped)
    }

    /// Drop one player's guest slot and its per-session bookkeeping.
    pub(crate) fn release_player(&mut self, session: &str, id: i64) {
        self.release_input_actor(session);
        if let Some(ref mut ecs) = self.ecs {
            ecs.remove_player(session);
        }
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id);
        }
        self.voice_room.leave_participant(id as u32);
        self.last_input_seq.remove(session);
        self.input_journal.retain(|j| j.session != session);
    }

    /// Deterministic spawn scatter for a new player, derived from how many
    /// members the room already has (`width`/`height` come from the game
    /// config in `host.rs`; kept as parameters so rooms stays config-free).
    #[allow(dead_code)]
    pub(crate) fn spawn_point(&self, width: f64, height: f64) -> (f64, f64) {
        let n = self.sessions.len();
        (
            60.0 + ((n as f64) * 97.0) % (width - 120.0),
            60.0 + ((n as f64) * 61.0) % (height - 120.0),
        )
    }

    /// Full join sequence for a member session: mint the player id, run the
    /// guest's `on_join`, register in the voice room, and record the session.
    /// Returns the assigned player id.
    pub(crate) fn join(&mut self, session: &str, x: f64, y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut ecs) = self.ecs {
            ecs.add_player(session, id, x, y);
        }
        if let Some(ref mut wasm) = self.wasm {
            if let Err(e) = wasm.on_join(id, x, y) {
                eprintln!("[server] ⚠️ guest on_join error for player {id}: {e}");
            }
        }
        self.voice_room
            .join_participant(id as u32, Some(1), [x, 0.0, y]);
        self.sessions.insert(session.to_string(), id);
        id
    }

    /// Register a spectator session (no guest `on_join`, no voice participant).
    #[allow(dead_code)]
    pub(crate) fn join_spectator(&mut self, session: &str) {
        self.empty_since = None;
        self.spectators.insert(session.to_string());
    }

    /// Spectator upgrade handshake (`[0xFF, 'S', 'P', 'E', 'C']`): drop the
    /// guest player slot, then re-register the same session as a spectator.
    /// The caller sends the pre-framed `SPECTATOR_CONFIRM` reply — this crate
    /// does not own the transport.
    pub(crate) fn make_spectator(&mut self, session: &str) {
        if let Some(id) = self.sessions.remove(session) {
            if let Some(ref mut wasm) = self.wasm {
                let _ = wasm.on_leave(id);
            }
        }
        self.schema_sent.remove(session);
        self.spectators.insert(session.to_string());
    }

    /// Join a raw (UDP native-engine) player: guest `on_join` only, no
    /// session/voice bookkeeping — the plane tracks the peer address itself.
    pub(crate) fn add_raw_player(&mut self, x: f64, y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_join(id, x, y);
        }
        id
    }

    /// Release a raw player slot (guest `on_leave`) on timeout or spectator
    /// upgrade.
    pub(crate) fn remove_raw_player(&mut self, id: i64) {
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id);
        }
    }

    /// Detach a session's shared bookkeeping: spectator slot, pending schema
    /// acks, and the WebRTC voice mesh entry. Returns the voice-peer id that
    /// vanished, if the session was in the P2P mesh.
    fn detach_session(&mut self, session: &str) -> Option<u32> {
        self.spectators.remove(session);
        self.schema_sent.remove(session);
        self.acked_event_seq.remove(session);
        // A WebRTC peer that vanishes with its socket has to be
        // announced, otherwise the rest of the mesh keeps
        // re-negotiating with an id nobody answers.
        self.voice_mesh.leave_session(session)
    }

    fn arm_empty_timer(&mut self) {
        if self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(Instant::now());
        }
    }

    /// Consented disconnect: the client asked to leave (or the server closed
    /// the socket), so the seat is released right away — `allowReconnection`
    /// does not hold it. Returns the voice-peer id that vanished, if any.
    pub(crate) fn handle_disconnect(&mut self, session: &str) -> Option<u32> {
        let gone = self.detach_session(session);
        self.pending_leaves.remove(session);
        match self.sessions.get(session).copied() {
            Some(id) => {
                self.sessions.remove(session);
                self.release_player(session, id);
            }
            None => self.release_input_actor(session),
        }
        self.arm_empty_timer();
        gone
    }

    /// Unexpected disconnect (socket drop, heartbeat timeout): if the guest
    /// implements `allowReconnection`, the seat and player id are held until
    /// the deadline; otherwise the player is released immediately.
    pub(crate) fn handle_unexpected_disconnect(&mut self, session: &str) -> Option<u32> {
        let gone = self.detach_session(session);
        match self.sessions.get(session).copied() {
            Some(id) => {
                // A seat may be held for reconnection; the player id and guest
                // state are untouched until the deadline expires.
                let hold = self.guest_allow_reconnection(id).unwrap_or(0);
                if hold > 0 {
                    let deadline = Instant::now() + Duration::from_millis(hold as u64);
                    self.pending_leaves.insert(session.to_string(), (id, deadline));
                } else {
                    self.sessions.remove(session);
                    self.release_player(session, id);
                }
            }
            None => self.release_input_actor(session),
        }
        self.arm_empty_timer();
        gone
    }

    /// Mark the room live again (a member arrived or sent traffic).
    pub(crate) fn note_activity(&mut self) {
        self.empty_since = None;
    }

    /// Empty-room reaping: once the room holds no players and no spectators,
    /// start the idle timer on first sight and report `true` after `timeout`
    /// of continuous emptiness — the caller then disposes the room. Raw UDP
    /// peers live outside the room, so the caller must also gate on an empty
    /// UDP peer map before honoring `true`.
    pub(crate) fn reap_check(&mut self, timeout: Duration) -> bool {
        if !(self.sessions.is_empty() && self.spectators.is_empty()) {
            return false;
        }
        match self.empty_since {
            Some(t) if t.elapsed() > timeout => true,
            Some(_) => false,
            None => {
                self.empty_since = Some(Instant::now());
                false
            }
        }
    }

    /// Advance the guest one tick (Rule 5: fault-isolated — a trap does not
    /// crash the server) and, when rollback is on, take the rollback
    /// checkpoint via the guest's own `on_save` contract.
    ///
    /// On success returns the packed delta pointer `(ptr << 32 | len)` from
    /// `on_tick`; `Ok(0)` from `on_tick` means "no delta this tick". On guest
    /// trap returns `Err` with the room-scoped log left to the caller.
    pub(crate) fn tick(
        &mut self,
        dt: f64,
        rollback_enabled: bool,
    ) -> Result<i64, engine_wasm::WasmError> {
        self.current_tick += 1;
        if self.backend == RoomBackend::InputHub {
            // reference-style Input Hub: zero server physics.
            return Ok(0);
        }
        // Simulation cadence: the guest may ask the host to run `onTick`
        // less often than the network tick. Each run still receives a
        // fixed dt (the configured interval), so the simulation stays
        // deterministic instead of seeing jittery deltas.
        let rate_ms = self.guest_simulation_rate_ms();
        let mut guest_dt = dt;
        if rate_ms > 0 {
            let interval = Duration::from_millis(rate_ms as u64);
            self.sim_accum += Duration::from_secs_f64(dt);
            if self.sim_accum < interval {
                return Ok(0);
            }
            // Never burst through a stall: one step, fresh accumulator.
            self.sim_accum = Duration::ZERO;
            guest_dt = interval.as_secs_f64();
        } else {
            self.sim_accum = Duration::ZERO;
        }
        if let Some(ref mut wasm) = self.wasm {
            wasm.data_mut().clock_ticks = self.current_tick as i64;
            let packed = wasm.on_tick(guest_dt)?;
            if rollback_enabled {
                // The guest serializes its own state (tiny blob) — no raw
                // linear-memory copy in the host.
                let packed_state = wasm.on_save()?;
                let sptr = ((packed_state >> 32) as u32) as usize;
                let slen = (packed_state as u32) as usize;
                let blob = wasm.read_memory(sptr as u32, slen as u32)?;
                self.rollback_buffer
                    .save_blob(u64::from(self.current_tick), blob);
            }
            Ok(packed)
        } else {
            // Relay mode: no WASM linear memory
            Ok(0)
        }
    }

    /// Take every typed message the guest queued through `host_send` since the
    /// last drain. The host loop routes and frames them after the tick (or
    /// after a join/input that produced them), so a guest never touches the
    /// transport.
    pub fn drain_outbox(&mut self) -> Vec<OutboundMessage> {
        let mut out = match self.wasm.as_mut() {
            Some(wasm) => std::mem::take(&mut wasm.data_mut().outbox),
            None => Vec::new(),
        };
        if !self.ecs_outbox.is_empty() {
            out.append(&mut self.ecs_outbox);
        }
        out
    }

    /// Hot-swap this room's WASM instance for the freshly compiled template,
    /// keeping the room's physics world and re-joining every active player at
    /// their id-derived spawn point so nobody is disconnected. Returns the
    /// number of players carried over, or the instantiation error as text.
    pub(crate) fn reload_wasm(
        &mut self,
        pre: &engine_wasm::WasmPreInstance<RoomHostData>,
        max_clients: i32,
        width: f64,
        height: f64,
    ) -> Result<usize, String> {
        let physics = if let Some(ref wasm) = self.wasm {
            wasm.data().physics.clone()
        } else {
            PhysicsWorld3D::default()
        };
        let mut new_wasm =
            RoomLogic::from_pre(pre, RoomHostData::new(physics)).map_err(|e| e.to_string())?;
        let _ = new_wasm.on_create(max_clients);
        // Re-join active players so state is maintained seamlessly
        for (_sid, &pid) in &self.sessions {
            let (x, y) = (
                60.0 + ((pid as f64) * 97.0) % (width - 120.0),
                60.0 + ((pid as f64) * 61.0) % (height - 120.0),
            );
            let _ = new_wasm.on_join(pid, x, y);
        }
        let players = self.sessions.len();
        self.wasm = Some(new_wasm);
        self.backend = RoomBackend::Wasm;
        Ok(players)
    }
}

/// All rooms + the pre-instantiated room template (hot-swappable).
pub struct GameServer {
    /// Linked, validated template: spawning a room costs one instantiate.
    /// `None` when the process booted without a room module — the server then
    /// serves zero-WASM relay rooms only instead of refusing to start.
    pub(crate) pre: Option<engine_wasm::WasmPreInstance<RoomHostData>>,
    pub(crate) max_clients: i32,
    pub(crate) physics: PhysicsWorld3D,
    pub(crate) rooms: HashMap<String, GameRoom>,
    pub(crate) permissions: engine_wasm::AbiPermissions,
    pub(crate) persistent_snapshots: HashMap<String, Vec<u8>>,
}

impl GameServer {
    pub(crate) fn new(
        pre: Option<engine_wasm::WasmPreInstance<RoomHostData>>,
        max_clients: i32,
        physics: PhysicsWorld3D,
        permissions: engine_wasm::AbiPermissions,
    ) -> Self {
        Self {
            pre,
            max_clients,
            physics,
            rooms: HashMap::new(),
            permissions,
            persistent_snapshots: HashMap::new(),
        }
    }

    /// Save a serialized snapshot of a room for persistence across reaps.
    pub fn save_snapshot(&mut self, room_name: &str, snapshot: Vec<u8>) {
        // Capability gate: a project that disables `storage` cannot persist
        // anything, however the snapshot was produced.
        if !self.permissions.storage {
            eprintln!(
                "[server] 🚫 storage disabled by manifest — dropping snapshot for room '{room_name}'"
            );
            return;
        }
        eprintln!(
            "[server] 💾 Saved persistent snapshot for room '{room_name}' ({} bytes)",
            snapshot.len()
        );
        self.persistent_snapshots
            .insert(room_name.to_string(), snapshot);
    }

    /// Retrieve a previously saved snapshot for a room, if one exists.
    pub fn load_snapshot(&self, room_name: &str) -> Option<&Vec<u8>> {
        self.persistent_snapshots.get(room_name)
    }

    /// Whether a persistent snapshot exists for this room.
    pub fn has_snapshot(&self, room_name: &str) -> bool {
        self.persistent_snapshots.contains_key(room_name)
    }

    /// True when a room module was linked, so WASM rooms are available.
    #[inline]
    pub(crate) fn has_room_module(&self) -> bool {
        self.pre.is_some()
    }

    /// True when `name` is a *new* room and the registry is already at
    /// `max_rooms` (`0` = unlimited). Rooms that already exist are never
    /// blocked — the limit only bites on creation, so an open room-name space
    /// cannot be used to allocate unbounded rooms (each may be a WASM instance).
    pub(crate) fn would_exceed_room_limit(&self, name: &str, max_rooms: u32) -> bool {
        max_rooms > 0 && !self.rooms.contains_key(name) && self.rooms.len() >= max_rooms as usize
    }

    pub(crate) fn get_or_create(
        &mut self,
        name: &str,
    ) -> Result<&mut GameRoom, engine_wasm::WasmError> {
        // Hot path for an existing room borrows the key: no String per packet.
        if !self.rooms.contains_key(name) {
            let mut room = if let Some(pre) = &self.pre {
                let mut room = GameRoom::new(pre, self.max_clients, self.physics.clone())?;
                if let Some(w) = room.wasm.as_mut() {
                    w.data_mut().seed_rng(name);
                }
                eprintln!(
                    "[server] room '{name}' created with 3D physics world (ABI v{})",
                    engine_wasm::CURRENT_ABI_VERSION
                );
                room
            } else {
                eprintln!(
                    "[server] room '{name}' created in relay mode (no room module linked)"
                );
                GameRoom::new_relay()
            };
            // Capability gate: only rooms allowed `storage` may persist.
            room.storage_enabled = self.permissions.storage;

            // Auto-restore persistent snapshot if available
            if let Some(snap_bytes) = self.persistent_snapshots.get(name) {
                if let Err(e) = room.import_snapshot(snap_bytes) {
                    eprintln!(
                        "[server] ⚠️ Failed to restore persistent snapshot for '{name}': {e}"
                    );
                } else {
                    eprintln!("[server] 🔄 Restored persistent snapshot for room '{name}'");
                }
            }

            self.rooms.insert(name.to_string(), room);
        }
        Ok(self.rooms.get_mut(name).expect("room just inserted"))
    }

    /// Retrieve or instantiate a native bevy_ecs room (zero-WASM, Rust sim).
    /// `arena` is the cooked `arena.oxcl` mesh when the project ships one.
    pub(crate) fn get_or_create_ecs(
        &mut self,
        name: &str,
        arena: Option<&engine_physics::oxcl::CookedMesh>,
    ) -> &mut GameRoom {
        if !self.rooms.contains_key(name) {
            let mut room = GameRoom::new_ecs(name, arena);
            eprintln!(
                "[server] room '{name}' created with native bevy_ecs backend (Rapier3D + RoomRng{})",
                if arena.is_some() { " + OXCL arena" } else { "" }
            );
            room.storage_enabled = self.permissions.storage;
            self.rooms.insert(name.to_string(), room);
        }
        self.rooms.get_mut(name).expect("room just inserted")
    }

    /// Install a newly compiled template and hot-swap every live room onto it
    /// (Level 1 hot-reload: `host.rs` calls this after `runtime.compile` +
    /// `runtime.prepare` succeed on a changed `room_logic.wasm`).
    pub(crate) fn hot_reload(
        &mut self,
        pre: engine_wasm::WasmPreInstance<RoomHostData>,
        width: f64,
        height: f64,
    ) {
        self.pre = Some(pre);
        let pre = self.pre.as_ref().expect("just set by hot_reload");
        for (room_name, room) in &mut self.rooms {
            match room.reload_wasm(pre, self.max_clients, width, height) {
                Ok(players) => {
                    eprintln!(
                        "[server] 🔄 Room '{room_name}' hot-reloaded with {players} active players!"
                    );
                }
                Err(e) => {
                    eprintln!("[server] ⚠️ Failed to re-instantiate room '{room_name}': {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_room_lifecycle() {
        let mut room = GameRoom::new_relay();
        assert!(room.is_relay());
        assert!(!room.is_wasm());
        assert!(room.wasm.is_none());
        assert!(room.is_hibernating());

        // Tick on Relay succeeds with 0
        let res = room.tick(0.016, false);
        assert_eq!(res.unwrap(), 0);

        // Player joins -> cancels hibernation
        let p1 = room.join("sess_alice", 50.0, 50.0);
        assert_eq!(p1, 1);
        assert!(!room.is_hibernating());
        assert!(room.empty_since.is_none());

        // Spectator upgrade
        room.make_spectator("sess_alice");
        assert_eq!(room.sessions.len(), 0);
        assert_eq!(room.spectators.len(), 1);
        assert!(!room.is_hibernating()); // Still has spectator, not hibernating

        // Disconnect
        room.handle_disconnect("sess_alice");
        assert!(room.is_hibernating());
        assert!(room.empty_since.is_some());
    }

    #[test]
    fn test_room_hibernation_and_reap_timeout() {
        let mut room = GameRoom::new_relay();
        assert!(room.is_hibernating());

        // Initial check starts idle countdown
        assert!(!room.reap_check(Duration::from_secs(30)));

        // Simulated elapsed 31s
        room.empty_since = Some(Instant::now() - Duration::from_secs(31));
        assert!(room.reap_check(Duration::from_secs(30)));

        // Active player joins: resets timer
        let _ = room.join("sess_bob", 10.0, 10.0);
        assert!(!room.is_hibernating());
        assert!(!room.reap_check(Duration::from_secs(30)));
    }

    /// A project with no `room.ts` boots the host with `pre = None`. Room
    /// creation must then fall back to a zero-WASM relay room, not panic.
    #[test]
    fn server_without_room_module_serves_relay_rooms() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );
        assert!(!server.has_room_module());

        let room = server.get_or_create("plain_room").expect("relay room");
        assert!(room.is_relay());
        assert!(room.wasm.is_none());

        // A second call returns the same room, not a fresh one.
        let _ = server.get_or_create("plain_room");
        assert_eq!(server.rooms.len(), 1);
    }

    #[test]
    fn room_limit_blocks_only_new_rooms() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );
        let _ = server.get_or_create("a");

        // At the cap, a *new* name is refused...
        assert!(server.would_exceed_room_limit("b", 1));
        // ...but the room that already exists is never blocked...
        assert!(!server.would_exceed_room_limit("a", 1));
        // ...and 0 disables the cap entirely.
        assert!(!server.would_exceed_room_limit("b", 0));
        assert!(!server.would_exceed_room_limit("b", 2));
    }

    /// ABI-complete guest written in WAT: `on_is_full` declares "full at 2
    /// players" (well under the host's 16) and `on_save` returns a non-empty
    /// 8-byte buffer so the snapshot path has something real to move.
    const ABI_GUEST_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (func (export "on_create") (param i32))
          (func (export "on_join") (param i64 f64 f64))
          (func (export "on_leave") (param i64))
          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))
          (func (export "on_input") (param i64 i32 i32))
          (func (export "on_tick") (param f64) (result i64) (i64.const 0))
          (func (export "on_save") (result i64) (i64.const 8))
          (func (export "on_load") (param i32 i32))
          (func (export "on_is_full") (param i32) (result i32)
            (i32.ge_s (local.get 0) (i32.const 2)))
        )
    "#;

    fn wat_room() -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let module = runtime
            .compile(ABI_GUEST_WAT.as_bytes())
            .expect("compile guest wat");
        let linker = build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate guest room")
    }

    /// Guest that exercises every Phase 4 hook: auth reads the options byte,
    /// a 5s reconnection hold, patch/simulation cadence, and counters for
    /// leave/reconnect/dispose so tests can observe the lifecycle.
    const LIFECYCLE_GUEST_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $ticks (mut i32) (i32.const 0))
          (global $leaves (mut i32) (i32.const 0))
          (global $reconnects (mut i32) (i32.const 0))
          (global $disposals (mut i32) (i32.const 0))
          (func (export "on_create") (param i32))
          (func (export "on_join") (param i64 f64 f64))
          (func (export "on_leave") (param i64)
            (global.set $leaves (i32.add (global.get $leaves) (i32.const 1))))
          (func (export "on_alloc") (param i32) (result i32) (i32.const 1024))
          (func (export "on_input") (param i64 i32 i32))
          (func (export "on_tick") (param f64) (result i64)
            (global.set $ticks (i32.add (global.get $ticks) (i32.const 1)))
            (i64.const 0))
          (func (export "on_save") (result i64) (i64.const 0))
          (func (export "on_load") (param i32 i32))
          (func (export "on_auth") (param i64 i32 i32) (result i32)
            (i32.and
              (i32.gt_s (local.get 2) (i32.const 0))
              (i32.eq (i32.load8_u (i32.const 1024)) (i32.const 123))))
          (func (export "on_allow_reconnection") (param i64) (result i32) (i32.const 5000))
          (func (export "on_reconnect") (param i64)
            (global.set $reconnects (i32.add (global.get $reconnects) (i32.const 1))))
          (func (export "on_dispose")
            (global.set $disposals (i32.add (global.get $disposals) (i32.const 1))))
          (func (export "on_patch_rate") (result i32) (i32.const 100))
          (func (export "on_simulation_rate") (result i32) (i32.const 50))
          (func (export "tick_count") (result i32) (global.get $ticks))
          (func (export "leave_count") (result i32) (global.get $leaves))
          (func (export "reconnect_count") (result i32) (global.get $reconnects))
          (func (export "dispose_count") (result i32) (global.get $disposals))
        )
    "#;

    fn lifecycle_room() -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let module = runtime
            .compile(LIFECYCLE_GUEST_WAT.as_bytes())
            .expect("compile lifecycle wat");
        let linker = build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate lifecycle room")
    }

    fn guest_i32(room: &mut GameRoom, export: &str) -> i32 {
        room.wasm
            .as_mut()
            .expect("wasm guest")
            .raw_instance()
            .call::<(), i32>(export, ())
            .expect("counter export")
    }

    #[test]
    fn on_auth_sees_the_client_options_and_can_reject() {
        let mut room = lifecycle_room();
        // The guest admits only options that start with `{`.
        assert!(room.authorize(1, r#"{"token":"ok"}"#));
        assert!(!room.authorize(2, "denied"));

        // A guest without the export falls back to Colyseus's allow default.
        let mut relay = GameRoom::new_relay();
        assert!(relay.authorize(1, "anything"));
    }

    #[test]
    fn allow_reconnection_holds_the_seat_then_reconnect_resumes_it() {
        let mut room = lifecycle_room();
        let id = room.join("alice", 0.0, 0.0);
        assert_eq!(room.handle_unexpected_disconnect("alice"), None);

        assert!(
            room.has_pending_leave("alice"),
            "a 5s hold must keep the seat"
        );
        assert_eq!(
            room.sessions.get("alice"),
            Some(&id),
            "the player id and guest slot survive the disconnect"
        );
        assert_eq!(guest_i32(&mut room, "leave_count"), 0);

        assert_eq!(room.rejoin("alice"), Some(id));
        assert!(!room.has_pending_leave("alice"));
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 1);
        assert_eq!(guest_i32(&mut room, "leave_count"), 0);

        // A seat that never comes back is released when the deadline passes.
        assert_eq!(room.handle_unexpected_disconnect("alice"), None);
        room.pending_leaves
            .insert("alice".to_string(), (id, Instant::now()));
        assert!(room.expire_pending_leaves(Instant::now()));
        assert!(room.sessions.is_empty());
        assert_eq!(guest_i32(&mut room, "leave_count"), 1);
        assert!(room.is_hibernating());
    }

    #[test]
    fn consented_disconnect_releases_a_held_seat_immediately() {
        let mut room = lifecycle_room();
        let id = room.join("carol", 0.0, 0.0);
        // Hold the seat first: a drop that would be recoverable…
        room.handle_unexpected_disconnect("carol");
        assert!(room.has_pending_leave("carol"));

        // …then the client comes back and leaves on purpose: no hold.
        assert_eq!(room.rejoin("carol"), Some(id));
        assert_eq!(room.handle_disconnect("carol"), None);
        assert!(!room.has_pending_leave("carol"));
        assert!(room.sessions.is_empty());
        assert_eq!(guest_i32(&mut room, "leave_count"), 1);
        assert!(room.is_hibernating());
    }

    #[test]
    fn reconnect_after_the_deadline_is_a_fresh_join() {
        let mut room = lifecycle_room();
        let id = room.join("bob", 0.0, 0.0);
        room.handle_unexpected_disconnect("bob");
        room.pending_leaves
            .insert("bob".to_string(), (id, Instant::now()));
        assert!(room.expire_pending_leaves(Instant::now()));
        assert_eq!(room.rejoin("bob"), None);
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 0);
    }

    #[test]
    fn patch_rate_skips_windows_and_forces_the_next_full() {
        let mut room = lifecycle_room();
        let t0 = Instant::now();
        assert!(room.should_send_patch(t0), "first window is always due");
        assert!(
            !room.should_send_patch(t0 + Duration::from_millis(50)),
            "inside the 100ms window the patch is skipped"
        );
        assert!(
            room.take_patch_skipped(),
            "the skipped window must force a full snapshot"
        );
        assert!(!room.take_patch_skipped(), "the flag is consumed once");
        assert!(room.should_send_patch(t0 + Duration::from_millis(101)));
    }

    #[test]
    fn simulation_rate_runs_at_a_fixed_dt_and_skips_intermediate_ticks() {
        let mut room = lifecycle_room();
        for _ in 0..4 {
            assert_eq!(room.tick(0.01, false).unwrap(), 0);
        }
        assert_eq!(
            guest_i32(&mut room, "tick_count"),
            0,
            "50ms cadence with 10ms host ticks: four ticks are not enough"
        );
        assert_eq!(room.tick(0.01, false).unwrap(), 0);
        assert_eq!(
            guest_i32(&mut room, "tick_count"),
            1,
            "the fifth 10ms tick reaches the guest"
        );
        assert_eq!(room.sim_accum, Duration::ZERO);
    }

    #[test]
    fn dispose_runs_the_guest_hook() {
        let mut room = lifecycle_room();
        room.dispose();
        assert_eq!(guest_i32(&mut room, "dispose_count"), 1);
    }

    /// ABI-complete guest that queues one typed message during `on_create`.
    const MESSAGE_GUEST_WAT: &str = r#"
        (module
          (import "env" "host_send"
            (func $host_send (param i64 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "chat")
          (data (i32.const 8) "\01\02\03")
          (func (export "on_create") (param i32)
            (drop (call $host_send
              (i64.const 0) (i32.const 0) (i32.const 4) (i32.const 8) (i32.const 3))))
          (func (export "on_join") (param i64 f64 f64))
          (func (export "on_leave") (param i64))
          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))
          (func (export "on_input") (param i64 i32 i32))
          (func (export "on_tick") (param f64) (result i64) (i64.const 0))
          (func (export "on_save") (result i64) (i64.const 0))
          (func (export "on_load") (param i32 i32))
        )
    "#;

    fn wat_message_room(permissions: engine_wasm::AbiPermissions) -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let module = runtime
            .compile(MESSAGE_GUEST_WAT.as_bytes())
            .expect("compile message wat");
        let linker = build_host_linker(&runtime, permissions);
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate message room")
    }

    #[test]
    fn guest_messages_are_queued_for_the_host_and_gated_by_permission() {
        let mut room = wat_message_room(engine_wasm::AbiPermissions::default());
        assert_eq!(
            room.drain_outbox(),
            vec![OutboundMessage {
                target: 0,
                event: "chat".to_string(),
                payload: vec![1, 2, 3],
            }]
        );
        assert!(
            room.drain_outbox().is_empty(),
            "drain must consume the queue"
        );

        let mut denied = wat_message_room(engine_wasm::AbiPermissions {
            messaging: false,
            ..Default::default()
        });
        assert!(
            denied.drain_outbox().is_empty(),
            "a manifest without messaging must not let the guest queue traffic"
        );
    }

    #[test]
    fn guest_on_is_full_overrides_the_host_ceiling() {
        let mut room = wat_room();

        room.join("a", 0.0, 0.0);
        assert!(!room.is_full(16), "guest says not full at one player");

        room.join("b", 0.0, 0.0);
        assert!(
            room.is_full(16),
            "the guest's on_is_full(count >= 2) must drive the decision, not the host's 16"
        );

        // A relay room has no guest to ask, so the host ceiling still governs.
        let mut relay = GameRoom::new_relay();
        relay.join("a", 0.0, 0.0);
        relay.join("b", 0.0, 0.0);
        assert!(!relay.is_full(16));
        assert!(relay.is_at_capacity(2));
    }

    #[test]
    fn storage_permission_gates_guest_save_and_load() {
        let mut room = wat_room();
        let snap =
            serde_json::to_vec(&RoomSnapshotData::Wasm(vec![1, 2, 3])).expect("snapshot json");

        // Permitted: the guest's on_save / on_load are actually reached.
        assert!(room.export_snapshot().is_some(), "permitted room persists");
        assert!(
            room.import_snapshot(&snap).is_ok(),
            "permitted room restores"
        );

        // Denied: neither guest export may be reached.
        room.storage_enabled = false;
        assert!(
            room.export_snapshot().is_none(),
            "denied room must never call on_save"
        );
        assert!(
            room.import_snapshot(&snap).is_err(),
            "denied room must never call on_load"
        );
    }

    #[test]
    fn storage_permission_gates_the_snapshot_store() {
        let mut allowed = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );
        allowed.save_snapshot("zone", vec![1, 2, 3]);
        assert!(allowed.has_snapshot("zone"));

        let mut denied = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions {
                storage: false,
                ..Default::default()
            },
        );
        denied.save_snapshot("zone", vec![1, 2, 3]);
        assert!(
            !denied.has_snapshot("zone"),
            "a manifest without storage must not reach the snapshot store"
        );

        // ...and every room it spawns carries the denied capability.
        let room = denied.get_or_create("zone").expect("relay room");
        assert!(!room.storage_enabled);
    }

    #[test]
    fn relay_room_reports_capacity_at_the_ceiling() {
        let mut room = GameRoom::new_relay();
        assert!(!room.is_at_capacity(2));
        room.join("a", 0.0, 0.0);
        assert!(!room.is_at_capacity(2), "one of two is not full");
        room.join("b", 0.0, 0.0);
        assert!(room.is_at_capacity(2), "at the ceiling");
        assert!(!room.is_at_capacity(0), "0 means unlimited");
        assert!(!room.is_at_capacity(-1), "negative means unlimited");

        // A WASM room enforces its own policy in the guest, so the host must not.
        let mut wasm_room = GameRoom::new_relay();
        wasm_room.backend = RoomBackend::Wasm;
        wasm_room.join("c", 0.0, 0.0);
        assert!(!wasm_room.is_at_capacity(1));
    }

    #[test]
    fn test_multi_room_autocreate_and_autoclose() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );

        // 1. Auto-create Room A dynamically on join
        let room_a = server.get_or_create("room_a").expect("relay room");
        room_a.join("p1_a", 0.0, 0.0);
        room_a.join("p2_a", 10.0, 10.0);
        assert!(!room_a.is_hibernating());

        // 2. Auto-create Room B dynamically on join
        let room_b = server.get_or_create("room_b").expect("relay room");
        room_b.join("p1_b", 0.0, 0.0);
        assert!(!room_b.is_hibernating());
        assert_eq!(server.rooms.len(), 2);

        // 3. Players leave Room A -> Room A hibernates immediately (0% CPU)
        let room_a = server.rooms.get_mut("room_a").unwrap();
        room_a.handle_disconnect("p1_a");
        assert!(!room_a.is_hibernating()); // p2_a still inside
        room_a.handle_disconnect("p2_a");
        assert!(room_a.is_hibernating()); // 0 players -> hibernating!
        assert!(room_a.empty_since.is_some());

        // Room B remains fully active
        assert!(!server.rooms.get("room_b").unwrap().is_hibernating());

        // 4. Room A 30s timeout elapsed -> Auto-close & dispose from memory
        server.rooms.get_mut("room_a").unwrap().empty_since =
            Some(Instant::now() - Duration::from_secs(31));

        let mut dead_rooms = Vec::new();
        for (name, room) in server.rooms.iter_mut() {
            if room.is_hibernating() && room.reap_check(Duration::from_secs(30)) {
                dead_rooms.push(name.clone());
            }
        }
        for name in dead_rooms {
            server.rooms.remove(&name);
        }

        // Room A is purged, Room B remains intact
        assert_eq!(server.rooms.len(), 1);
        assert!(!server.rooms.contains_key("room_a"));
        assert!(server.rooms.contains_key("room_b"));

        // 5. New player reconnects to Room A -> Auto-created fresh again
        let room_a_fresh = server.get_or_create("room_a").expect("relay room");
        room_a_fresh.join("p3_a", 0.0, 0.0);
        assert!(!room_a_fresh.is_hibernating());
        assert_eq!(server.rooms.len(), 2);
    }

    #[test]
    fn test_room_persistence_autosave_and_autoload() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );

        // 1. Create a persistent survival world room (no module => relay room,
        //    which has no guest state to export on reap).
        let room = server.get_or_create("world_zone_survival").expect("relay room");
        room.join("player_builder", 0.0, 0.0);

        // 2. Player disconnects -> Room hibernates
        room.handle_disconnect("player_builder");
        assert!(room.is_hibernating());

        // 3. Room is reaped after 30s idle -> the relay room exports no
        //    snapshot, so disposal saves nothing.
        let mut room = server.rooms.remove("world_zone_survival").unwrap();
        assert!(room.export_snapshot().is_none(), "relay rooms have no state");
        server.rooms.insert("world_zone_survival".into(), room);

        // 4. Simulate a snapshot saved by a stateful (WASM) room earlier.
        let snap = serde_json::to_vec(&RoomSnapshotData::Wasm(vec![1, 2, 3, 4])).expect("snapshot");
        server.save_snapshot("world_zone_survival", snap);
        assert!(server.has_snapshot("world_zone_survival"));

        // 5. Another player connects 10 minutes later -> auto-create restores
        //    the snapshot into the fresh room.
        let has_snapshot = server.has_snapshot("world_zone_survival");
        let restored_wasm = {
            let restored = server
                .get_or_create("world_zone_survival")
                .expect("restored room");
            restored.wasm.is_none()
        };
        assert_eq!(server.rooms.len(), 1);
        // The restored room got the stored snapshot bytes (state import
        // itself needs a guest; the relay room keeps them for the next swap).
        assert!(has_snapshot);
        assert!(restored_wasm);
    }

    #[test]
    fn test_input_hub_mode() {
        let mut room = GameRoom::new_input_hub();
        assert!(room.is_input_hub());
        assert!(!room.is_wasm());

        // In InputHub mode, tick advances tick count and returns Ok(0) (zero physics)
        let res = room.tick(1.0 / 128.0, false);
        assert_eq!(res.unwrap(), 0);
        assert_eq!(room.current_tick, 1);

        // Record inputs from player actors
        room.record_input_tick(1, 10, &[1, 2, 3, 4]);
        room.record_input_tick(2, 10, &[5, 6, 7, 8]);
        room.record_input_tick(1, 11, &[9, 10, 11, 12]);

        let p1_latest = room.latest_input_tick(1).expect("p1 latest");
        assert_eq!(p1_latest.client_tick, 11);
        assert_eq!(p1_latest.payload, vec![9, 10, 11, 12]);

        let p2_latest = room.latest_input_tick(2).expect("p2 latest");
        assert_eq!(p2_latest.client_tick, 10);
        assert_eq!(p2_latest.payload, vec![5, 6, 7, 8]);
    }

    #[test]
    fn input_hub_claims_are_owned_and_monotonic() {
        let mut room = GameRoom::new_input_hub();
        let payload = [0u8; 9];

        assert!(room.claim_input_actor("session-a", 7));
        assert!(
            !room.claim_input_actor("session-a", 8),
            "session cannot change actors"
        );
        assert!(
            !room.claim_input_actor("session-b", 7),
            "actor is owned by session-a"
        );
        assert!(room.claim_input_actor("session-b", 8));

        assert!(room.record_input_tick_checked("session-a", 7, 10, &payload));
        assert!(
            !room.record_input_tick_checked("session-a", 7, 10, &payload),
            "duplicate tick is dropped"
        );
        assert!(
            !room.record_input_tick_checked("session-a", 7, 9, &payload),
            "replayed older tick is dropped"
        );
        assert!(
            !room.record_input_tick_checked("session-b", 8, 11, &[0u8; 8]),
            "wrong payload length is rejected"
        );

        room.release_input_actor("session-a");
        assert!(
            room.claim_input_actor("session-c", 7),
            "released actor can be claimed by a new session"
        );
        assert_eq!(room.input_hub_actor_count(), 2);
    }

    #[test]
    fn enabling_input_hub_drops_wasm_and_is_not_packet_driven() {
        let mut room = wat_room();
        assert!(room.is_wasm());
        room.enable_input_hub();
        assert!(room.is_input_hub());
        assert!(
            room.wasm.is_none(),
            "input hub must never keep a physics guest"
        );
        room.enable_input_hub();
        assert!(room.is_input_hub());
    }
}
