//! Room registry + WASM orchestration.
//!
//! Owns `GameRoom` (one isolated WASM simulation + its members), the host
//! linker for the game-host ABI, and the `GameServer` room registry with its
//! hot-swappable pre-instantiated template. The per-tick broadcast loop lives
//! in `crate::host` and the delta/input plumbing in `crate::netcode`, both of
//! which compile against the pinned types below.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use engine_physics::PhysicsWorld3D;
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
    /// Pure Unity C# Netcode Mode: Zero WASM required.
    UnityRelay,
    /// Server-Authoritative WASM Mode: Runs Rapier3D physics & game logic.
    Wasm,
    /// reference-Style Deterministic Tick Input-Hub: Server never runs physics,
    /// collects player inputs, bundles them by tick, and forwards them.
    InputHub,
}

/// Network state for a Unity networked entity (e.g. CYGView)
#[derive(Clone, Debug, Default)]
pub struct EntityNetState {
    pub view_id: u32,
    pub owner_session: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub dirty: bool,
}

/// Pure Unity C# Netcode state stored on the server.
/// Zero WASM dependency!
#[derive(Clone, Debug, Default)]
pub struct UnityNetcodeState {
    /// view_id -> EntityNetState (Transforms & state)
    pub entities: HashMap<u32, EntityNetState>,
    /// view_id -> (var_id -> serialized binary value)
    pub sync_vars: HashMap<u32, HashMap<u16, Vec<u8>>>,
    /// Prefab spawn records: view_id -> (prefab_id, owner_session, spawn_payload)
    pub spawned_prefabs: HashMap<u32, (u32, String, Vec<u8>)>,
    /// Pending broadcast delta buffer for the current tick
    pub dirty_entities: HashSet<u32>,
}

impl UnityNetcodeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_transform(
        &mut self,
        view_id: u32,
        owner_session: &str,
        pos: [f32; 3],
        rot: [f32; 4],
        vel: [f32; 3],
    ) {
        let entry = self.entities.entry(view_id).or_insert_with(|| EntityNetState {
            view_id,
            owner_session: owner_session.to_string(),
            position: pos,
            rotation: rot,
            velocity: vel,
            dirty: true,
        });
        entry.position = pos;
        entry.rotation = rot;
        entry.velocity = vel;
        entry.dirty = true;
        self.dirty_entities.insert(view_id);
    }

    pub fn update_sync_var(&mut self, view_id: u32, var_id: u16, val: Vec<u8>) {
        self.sync_vars.entry(view_id).or_default().insert(var_id, val);
    }

    pub fn export_snapshot(&self) -> UnityPersistentSnapshot {
        let entities = self
            .entities
            .values()
            .map(|e| EntityNetSnapshot {
                view_id: e.view_id,
                owner_session: e.owner_session.clone(),
                position: e.position,
                rotation: e.rotation,
                velocity: e.velocity,
            })
            .collect();

        let mut sync_vars = Vec::new();
        for (&view_id, vars) in &self.sync_vars {
            for (&var_id, val) in vars {
                sync_vars.push((view_id, var_id, val.clone()));
            }
        }

        let mut spawned_prefabs = Vec::new();
        for (&view_id, (prefab_id, owner, payload)) in &self.spawned_prefabs {
            spawned_prefabs.push((view_id, *prefab_id, owner.clone(), payload.clone()));
        }

        UnityPersistentSnapshot {
            entities,
            sync_vars,
            spawned_prefabs,
        }
    }

    pub fn import_snapshot(&mut self, snap: UnityPersistentSnapshot) {
        for e in snap.entities {
            self.entities.insert(
                e.view_id,
                EntityNetState {
                    view_id: e.view_id,
                    owner_session: e.owner_session,
                    position: e.position,
                    rotation: e.rotation,
                    velocity: e.velocity,
                    dirty: true,
                },
            );
            self.dirty_entities.insert(e.view_id);
        }

        for (view_id, var_id, val) in snap.sync_vars {
            self.sync_vars.entry(view_id).or_default().insert(var_id, val);
        }

        for (view_id, prefab_id, owner, payload) in snap.spawned_prefabs {
            self.spawned_prefabs
                .insert(view_id, (prefab_id, owner, payload));
        }
    }
}

/// Serialized entity snapshot for persistent world storage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityNetSnapshot {
    pub view_id: u32,
    pub owner_session: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
}

/// Serialized snapshot of a Pure Unity Relay room.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnityPersistentSnapshot {
    pub entities: Vec<EntityNetSnapshot>,
    pub sync_vars: Vec<(u32, u16, Vec<u8>)>,
    pub spawned_prefabs: Vec<(u32, u32, String, Vec<u8>)>,
}

/// Encapsulates snapshot data across backends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RoomSnapshotData {
    UnityRelay(UnityPersistentSnapshot),
    Wasm(Vec<u8>),
}

/// One named room: an isolated simulation + its members.
pub struct GameRoom {
    pub(crate) backend: RoomBackend,
    /// Mirrors the manifest's `storage` capability. When `false` the guest's
    /// `on_save`/`on_load` are never invoked, so a game cannot write to (or read
    /// from) the snapshot store — the capability is actually enforced.
    pub(crate) storage_enabled: bool,
    pub(crate) wasm: Option<RoomLogic<PhysicsWorld3D>>,
    pub(crate) unity: Option<UnityNetcodeState>,
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
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

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
                move |mut caller: wasmtime::Caller<'_, PhysicsWorld3D>,
                 shoot_tick: i64,
                 ox: f64, oy: f64, oz: f64,
                 dx: f64, dy: f64, dz: f64,
                 max_dist: f64,
                 out_ptr: i32| -> i32 {
                    if !permissions.lag_compensation {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

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
    /// Spawn a room from the pre-instantiated template: one `instantiate`
    /// against the shared linker, then the guest's `on_create`.
    pub(crate) fn new(
        pre: &engine_wasm::WasmPreInstance<PhysicsWorld3D>,
        max_clients: i32,
        physics: PhysicsWorld3D,
    ) -> Result<Self, engine_wasm::WasmError> {
        // A guest that predates the ABI (e.g. missing `on_alloc`) must fail the
        // one room, not abort the whole process — the caller rejects the client.
        let mut wasm = RoomLogic::from_pre(pre, physics)?;
        if let Err(e) = wasm.on_create(max_clients) {
            eprintln!("[server] ⚠️ guest on_create error: {e}");
        }
        Ok(Self {
            backend: RoomBackend::Wasm,
            // Default on; `GameServer::get_or_create` overrides it from the
            // manifest permissions for every spawned room.
            storage_enabled: true,
            wasm: Some(wasm),
            unity: Some(UnityNetcodeState::default()),
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
            acked_event_seq: HashMap::new(),
            input_journal: VecDeque::new(),
            input_hub_records: VecDeque::new(),
            input_hub_actors: HashMap::new(),
            input_hub_sessions: HashMap::new(),
            input_hub_last_tick: HashMap::new(),
        })
    }

    /// Spawn a Pure Unity Netcode Relay room (zero-WASM mode).
    pub(crate) fn new_unity_relay() -> Self {
        Self {
            backend: RoomBackend::UnityRelay,
            storage_enabled: true,
            wasm: None,
            unity: Some(UnityNetcodeState::default()),
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
            acked_event_seq: HashMap::new(),
            input_journal: VecDeque::new(),
            input_hub_records: VecDeque::new(),
            input_hub_actors: HashMap::new(),
            input_hub_sessions: HashMap::new(),
            input_hub_last_tick: HashMap::new(),
        }
    }

    /// Spawn a reference-inspired Deterministic Input-Hub room (zero-physics
    /// mode). The WASM guest is not retained: an input hub must never run
    /// server physics, even if the room was first created from a WASM template.
    pub fn new_input_hub() -> Self {
        let mut room = Self::new_unity_relay();
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
        self.input_hub_last_tick.insert(player_actor_id, client_tick);
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

    /// Whether this room runs in pure Unity C# relay mode.
    #[inline]
    pub fn is_unity_relay(&self) -> bool {
        self.backend == RoomBackend::UnityRelay
    }

    /// Whether this room runs in server-authoritative WASM mode.
    #[inline]
    pub fn is_wasm(&self) -> bool {
        self.backend == RoomBackend::Wasm
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
            RoomBackend::UnityRelay => {
                if let Some(ref u) = self.unity {
                    let snap = RoomSnapshotData::UnityRelay(u.export_snapshot());
                    serde_json::to_vec(&snap).ok()
                } else {
                    None
                }
            }
            RoomBackend::Wasm => {
                if let Some(ref mut wasm) = self.wasm {
                    if let Ok(packed) = wasm.on_save() {
                        let ptr = ((packed >> 32) as u32) as usize;
                        let len = (packed as u32) as usize;
                        if len > 0 {
                            let mut buf = vec![0u8; len];
                            if wasm.read_memory_into(ptr as u32, len as u32, &mut buf).is_ok() {
                                let snap = RoomSnapshotData::Wasm(buf);
                                return serde_json::to_vec(&snap).ok();
                            }
                        }
                    }
                }
                None
            }
            RoomBackend::InputHub => None,
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
            RoomSnapshotData::UnityRelay(unity_snap) => {
                if let Some(ref mut u) = self.unity {
                    u.import_snapshot(unity_snap);
                    Ok(())
                } else {
                    Err("Room has no UnityNetcodeState".into())
                }
            }
            RoomSnapshotData::Wasm(wasm_bytes) => {
                if let Some(ref mut wasm) = self.wasm {
                    let len = wasm_bytes.len() as i32;
                    let ptr = wasm.on_alloc(len).map_err(|e| format!("on_alloc failed: {e}"))?;
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

    /// The host's own ceiling: a Unity-relay room has no guest logic, so without
    /// this it would silently overfill — ignoring `max_clients` entirely.
    /// `max_clients <= 0` means "no limit". WASM rooms go through
    /// [`Self::is_full`], which lets the guest override the policy.
    #[inline]
    pub(crate) fn is_at_capacity(&self, max_clients: i32) -> bool {
        (self.is_unity_relay() || self.is_input_hub()) && max_clients > 0 && self.sessions.len() as i32 >= max_clients
    }

    /// Capacity decision for a joining player.
    ///
    /// A WASM room may ship an optional `on_is_full(count)` export to implement
    /// its own policy (teams, VIP slots, wave limits…); the host asks it and
    /// honours the answer. Rooms without that export — and pure Unity-relay
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
        let func = instance
            .get_typed_func::<(i32,), i32>("on_is_full")
            .ok()?;
        func.call(instance.store_mut(), (count,))
            .ok()
            .map(|v| v != 0)
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
        if let Some(ref mut wasm) = self.wasm {
            if let Err(e) = wasm.on_join(id, x, y) {
                eprintln!("[server] ⚠️ guest on_join error for player {id}: {e}");
            }
        }
        self.voice_room.join_participant(id as u32, Some(1), [x, 0.0, y]);
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

    /// Full disconnect sequence for one session: remove spectator and ack
    /// records, drop it from the WebRTC voice mesh, release the player slot
    /// (guest `on_leave`, voice participant, input seq, journal entries), and
    /// arm the empty-room timer when nobody is left.
    ///
    /// Returns the voice-peer id that vanished, if the session was in the P2P
    /// mesh — the caller must announce `PeerLeft` for it over its transport.
    pub(crate) fn handle_disconnect(&mut self, session: &str) -> Option<u32> {
        self.spectators.remove(session);
        self.acked_event_seq.remove(session);
        self.release_input_actor(session);
        // A WebRTC peer that vanishes with its socket has to be
        // announced, otherwise the rest of the mesh keeps
        // re-negotiating with an id nobody answers.
        let gone = self.voice_mesh.leave_session(session);
        if let Some(id) = self.sessions.remove(session) {
            if let Some(ref mut wasm) = self.wasm {
                let _ = wasm.on_leave(id);
            }
            self.voice_room.leave_participant(id as u32);
            self.last_input_seq.remove(session);
            self.input_journal.retain(|j| j.session != session);
        }
        if self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(Instant::now());
        }
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
        if let Some(ref mut wasm) = self.wasm {
            let packed = wasm.on_tick(dt)?;
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
            // Pure Unity relay: no WASM linear memory
            Ok(0)
        }
    }

    /// Hot-swap this room's WASM instance for the freshly compiled template,
    /// keeping the room's physics world and re-joining every active player at
    /// their id-derived spawn point so nobody is disconnected. Returns the
    /// number of players carried over, or the instantiation error as text.
    pub(crate) fn reload_wasm(
        &mut self,
        pre: &engine_wasm::WasmPreInstance<PhysicsWorld3D>,
        max_clients: i32,
        width: f64,
        height: f64,
    ) -> Result<usize, String> {
        let physics = if let Some(ref wasm) = self.wasm {
            wasm.data().clone()
        } else {
            PhysicsWorld3D::default()
        };
        let mut new_wasm = RoomLogic::from_pre(pre, physics).map_err(|e| e.to_string())?;
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
    /// serves Unity-relay rooms only instead of refusing to start.
    pub(crate) pre: Option<engine_wasm::WasmPreInstance<PhysicsWorld3D>>,
    pub(crate) max_clients: i32,
    pub(crate) physics: PhysicsWorld3D,
    pub(crate) rooms: HashMap<String, GameRoom>,
    pub(crate) permissions: engine_wasm::AbiPermissions,
    pub(crate) persistent_snapshots: HashMap<String, Vec<u8>>,
}

impl GameServer {
    pub(crate) fn new(
        pre: Option<engine_wasm::WasmPreInstance<PhysicsWorld3D>>,
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
        self.persistent_snapshots.insert(room_name.to_string(), snapshot);
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
        max_rooms > 0
            && !self.rooms.contains_key(name)
            && self.rooms.len() >= max_rooms as usize
    }

    pub(crate) fn get_or_create(
        &mut self,
        name: &str,
    ) -> Result<&mut GameRoom, engine_wasm::WasmError> {
        // Hot path for an existing room borrows the key: no String per packet.
        if !self.rooms.contains_key(name) {
            let mut room = if let Some(pre) = &self.pre {
                let room = GameRoom::new(pre, self.max_clients, self.physics.clone())?;
                eprintln!(
                    "[server] room '{name}' created with 3D physics world (ABI v{})",
                    engine_wasm::CURRENT_ABI_VERSION
                );
                room
            } else {
                eprintln!(
                    "[server] room '{name}' created in Unity-relay mode (no room module linked)"
                );
                GameRoom::new_unity_relay()
            };
            // Capability gate: only rooms allowed `storage` may persist.
            room.storage_enabled = self.permissions.storage;

            // Auto-restore persistent snapshot if available
            if let Some(snap_bytes) = self.persistent_snapshots.get(name) {
                if let Err(e) = room.import_snapshot(snap_bytes) {
                    eprintln!("[server] ⚠️ Failed to restore persistent snapshot for '{name}': {e}");
                } else {
                    eprintln!("[server] 🔄 Restored persistent snapshot for room '{name}'");
                }
            }

            self.rooms.insert(name.to_string(), room);
        }
        Ok(self.rooms.get_mut(name).expect("room just inserted"))
    }

    /// Retrieve or instantiate a Pure Unity C# Netcode Relay room (zero-WASM).
    pub(crate) fn get_or_create_unity(&mut self, name: &str) -> &mut GameRoom {
        if !self.rooms.contains_key(name) {
            let mut room = GameRoom::new_unity_relay();
            eprintln!(
                "[server] room '{name}' created with Pure Unity C# Netcode Relay (zero-WASM)",
            );
            room.storage_enabled = self.permissions.storage;

            // Auto-restore persistent snapshot if available
            if let Some(snap_bytes) = self.persistent_snapshots.get(name) {
                if let Err(e) = room.import_snapshot(snap_bytes) {
                    eprintln!("[server] ⚠️ Failed to restore persistent snapshot for '{name}': {e}");
                } else {
                    eprintln!("[server] 🔄 Restored persistent snapshot for room '{name}'");
                }
            }

            self.rooms.insert(name.to_string(), room);
        }
        self.rooms.get_mut(name).expect("room just inserted")
    }

    /// Install a newly compiled template and hot-swap every live room onto it
    /// (Level 1 hot-reload: `host.rs` calls this after `runtime.compile` +
    /// `runtime.prepare` succeed on a changed `room_logic.wasm`).
    pub(crate) fn hot_reload(
        &mut self,
        pre: engine_wasm::WasmPreInstance<PhysicsWorld3D>,
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
    fn test_unity_relay_room_lifecycle() {
        let mut room = GameRoom::new_unity_relay();
        assert!(room.is_unity_relay());
        assert!(!room.is_wasm());
        assert!(room.wasm.is_none());
        assert!(room.unity.is_some());
        assert!(room.is_hibernating());

        // Tick on UnityRelay succeeds with 0
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
        let mut room = GameRoom::new_unity_relay();
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

    #[test]
    fn test_unity_netcode_state_updates() {
        let mut state = UnityNetcodeState::default();
        state.update_transform(101, "sess_player1", [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [0.5, 0.0, 0.0]);

        let ent = state.entities.get(&101).expect("entity exists");
        assert_eq!(ent.position, [1.0, 2.0, 3.0]);
        assert_eq!(ent.owner_session, "sess_player1");
        assert!(state.dirty_entities.contains(&101));

        state.update_sync_var(101, 12, vec![0xCA, 0xFE]);
        let val = state.sync_vars.get(&101).and_then(|v| v.get(&12)).expect("sync var exists");
        assert_eq!(val, &[0xCA, 0xFE]);
    }

    /// A project that ships only Unity C# gameplay has no `room.ts`, so the
    /// host boots with `pre = None`. Room creation must then fall back to a
    /// Unity-relay room instead of panicking.
    #[test]
    fn server_without_room_module_serves_unity_relay_rooms() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );
        assert!(!server.has_room_module());

        let room = server.get_or_create("plain_room").expect("relay room");
        assert!(room.is_unity_relay());
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
        server.get_or_create_unity("a");

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
        let mut relay = GameRoom::new_unity_relay();
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
        assert!(room.import_snapshot(&snap).is_ok(), "permitted room restores");

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
        let mut room = GameRoom::new_unity_relay();
        assert!(!room.is_at_capacity(2));
        room.join("a", 0.0, 0.0);
        assert!(!room.is_at_capacity(2), "one of two is not full");
        room.join("b", 0.0, 0.0);
        assert!(room.is_at_capacity(2), "at the ceiling");
        assert!(!room.is_at_capacity(0), "0 means unlimited");
        assert!(!room.is_at_capacity(-1), "negative means unlimited");

        // A WASM room enforces its own policy in the guest, so the host must not.
        let mut wasm_room = GameRoom::new_unity_relay();
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
        let room_a = server.get_or_create_unity("room_a");
        room_a.join("p1_a", 0.0, 0.0);
        room_a.join("p2_a", 10.0, 10.0);
        assert!(!room_a.is_hibernating());

        // 2. Auto-create Room B dynamically on join
        let room_b = server.get_or_create_unity("room_b");
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
        server.rooms.get_mut("room_a").unwrap().empty_since = Some(Instant::now() - Duration::from_secs(31));

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
        let room_a_fresh = server.get_or_create_unity("room_a");
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

        // 1. Create a persistent survival world room
        let room = server.get_or_create_unity("world_zone_survival");
        room.join("player_builder", 0.0, 0.0);

        // 2. Player spawns a shelter (prefab_id = 42, view_id = 100)
        let unity = room.unity.as_mut().unwrap();
        unity.spawned_prefabs.insert(100, (42, "player_builder".into(), vec![1, 2, 3, 4]));
        unity.update_transform(100, "player_builder", [12.5, 3.0, -45.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        unity.update_sync_var(100, 1, vec![100, 100]); // Health = 100

        // 3. Player disconnects -> Room hibernates
        room.handle_disconnect("player_builder");
        assert!(room.is_hibernating());

        // 4. Room is reaped after 30s idle -> auto-save snapshot before disposal
        let mut room = server.rooms.remove("world_zone_survival").unwrap();
        let snapshot = room.export_snapshot().expect("export snapshot");
        server.save_snapshot("world_zone_survival", snapshot);

        // Verify room is gone from active memory
        assert_eq!(server.rooms.len(), 0);
        assert!(server.has_snapshot("world_zone_survival"));

        // 5. Another player connects to "world_zone_survival" 10 minutes later -> auto-create and auto-restore!
        server.get_or_create_unity("world_zone_survival");
        assert_eq!(server.rooms.len(), 1);

        // Verify world state has been restored 100%
        let restored_room = server.rooms.get("world_zone_survival").unwrap();
        let restored_unity = restored_room.unity.as_ref().unwrap();
        assert_eq!(restored_unity.entities.len(), 1);
        let entity = restored_unity.entities.get(&100).expect("view_id 100 preserved");
        assert_eq!(entity.position, [12.5, 3.0, -45.0]);
        assert_eq!(entity.rotation, [0.0, 1.0, 0.0, 0.0]);
        assert!(entity.dirty);

        // Verify spawned prefabs and sync vars restored
        assert!(restored_unity.spawned_prefabs.contains_key(&100));
        assert_eq!(restored_unity.spawned_prefabs.get(&100).unwrap().0, 42);
        assert_eq!(restored_unity.spawned_prefabs.get(&100).unwrap().2, vec![1, 2, 3, 4]);

        assert!(restored_unity.sync_vars.contains_key(&100));
        assert_eq!(restored_unity.sync_vars.get(&100).unwrap().get(&1).unwrap(), &vec![100, 100]);
    }

    #[test]
    fn test_input_hub_mode() {
        let mut room = GameRoom::new_input_hub();
        assert!(room.is_input_hub());
        assert!(!room.is_wasm());
        assert!(!room.is_unity_relay());

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
        assert!(!room.claim_input_actor("session-a", 8), "session cannot change actors");
        assert!(!room.claim_input_actor("session-b", 7), "actor is owned by session-a");
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
            "non-Unity payload length is rejected"
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
        assert!(room.wasm.is_none(), "input hub must never keep a physics guest");
        room.enable_input_hub();
        assert!(room.is_input_hub());
    }
}
