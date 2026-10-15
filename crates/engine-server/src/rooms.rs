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
pub struct GameRoom {
    pub(crate) wasm: RoomLogic<PhysicsWorld3D>,
    pub(crate) sessions: HashMap<String, i64>,
    pub(crate) spectators: HashSet<String>,
    pub(crate) next_player_id: i64,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) current_tick: u64,
    pub(crate) rollback_buffer: engine_wasm::RollbackRingBuffer,
    pub(crate) voice_room: engine_voice::VoiceRoom,
    /// WebRTC signaling membership: which voice peers exist and which socket each
    /// one is carried on. Separate from `voice_room` because a peer opts into the
    /// P2P mesh with a `Join` signal, while `voice_room` tracks everyone.
    pub(crate) voice_mesh: engine_voice::VoiceMesh,
    /// Highest client input seq applied per session — echoed back as `acked_seq`
    /// in the delta tail so clients can reconcile local prediction.
    pub(crate) last_input_seq: HashMap<String, u16>,
    pub(crate) event_seq: u16,
    /// Room-wide stamp on the unreliable delta channel: one per delta the room
    /// emits, shared by every recipient of that tick.
    pub(crate) delta_seq: u16,
    /// Highest event seq each client confirmed holding.
    pub(crate) acked_event_seq: HashMap<String, u16>,
    /// Recently applied guest inputs, oldest first, tagged with the tick they
    /// arrived in. The replay script for a late-input rollback.
    pub(crate) input_journal: VecDeque<JournalInput>,
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
    ) -> Self {
        let mut wasm = RoomLogic::from_pre(pre, physics).expect("instantiate room");
        if let Err(e) = wasm.on_create(max_clients) {
            eprintln!("[server] ⚠️ guest on_create error: {e}");
        }
        Self {
            wasm,
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
        }
    }

    /// Deterministic spawn scatter for a new player, derived from how many
    /// members the room already has (`width`/`height` come from the game
    /// config in `host.rs`; kept as parameters so rooms stays config-free).
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
        if let Err(e) = self.wasm.on_join(id, x, y) {
            eprintln!("[server] ⚠️ guest on_join error for player {id}: {e}");
        }
        self.voice_room.join_participant(id as u32, Some(1), [x, 0.0, y]);
        self.sessions.insert(session.to_string(), id);
        id
    }

    /// Register a spectator session (no guest `on_join`, no voice participant).
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
            let _ = self.wasm.on_leave(id);
        }
        self.spectators.insert(session.to_string());
    }

    /// Join a raw (UDP native-engine) player: guest `on_join` only, no
    /// session/voice bookkeeping — the plane tracks the peer address itself.
    pub(crate) fn add_raw_player(&mut self, x: f64, y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        let _ = self.wasm.on_join(id, x, y);
        id
    }

    /// Release a raw player slot (guest `on_leave`) on timeout or spectator
    /// upgrade.
    pub(crate) fn remove_raw_player(&mut self, id: i64) {
        let _ = self.wasm.on_leave(id);
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
        // A WebRTC peer that vanishes with its socket has to be
        // announced, otherwise the rest of the mesh keeps
        // re-negotiating with an id nobody answers.
        let gone = self.voice_mesh.leave_session(session);
        if let Some(id) = self.sessions.remove(session) {
            let _ = self.wasm.on_leave(id);
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
        let packed = self.wasm.on_tick(dt)?;
        if rollback_enabled {
            // The guest serializes its own state (tiny blob) — no raw
            // linear-memory copy in the host.
            let packed_state = self.wasm.on_save()?;
            let sptr = ((packed_state >> 32) as u32) as usize;
            let slen = (packed_state as u32) as usize;
            let blob = self.wasm.read_memory(sptr as u32, slen as u32)?;
            self.rollback_buffer
                .save_blob(u64::from(self.current_tick), blob);
        }
        Ok(packed)
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
        let physics = self.wasm.data().clone();
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
        self.wasm = new_wasm;
        Ok(players)
    }
}

/// All rooms + the pre-instantiated room template (hot-swappable).
pub struct GameServer {
    /// Linked, validated template: spawning a room costs one instantiate.
    pub(crate) pre: engine_wasm::WasmPreInstance<PhysicsWorld3D>,
    pub(crate) max_clients: i32,
    pub(crate) physics: PhysicsWorld3D,
    pub(crate) rooms: HashMap<String, GameRoom>,
    pub(crate) permissions: engine_wasm::AbiPermissions,
}

impl GameServer {
    pub(crate) fn new(
        pre: engine_wasm::WasmPreInstance<PhysicsWorld3D>,
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
        }
    }

    pub(crate) fn get_or_create(&mut self, name: &str) -> &mut GameRoom {
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

    /// Install a newly compiled template and hot-swap every live room onto it
    /// (Level 1 hot-reload: `host.rs` calls this after `runtime.compile` +
    /// `runtime.prepare` succeed on a changed `room_logic.wasm`).
    pub(crate) fn hot_reload(
        &mut self,
        pre: engine_wasm::WasmPreInstance<PhysicsWorld3D>,
        width: f64,
        height: f64,
    ) {
        self.pre = pre;
        for (room_name, room) in &mut self.rooms {
            match room.reload_wasm(&self.pre, self.max_clients, width, height) {
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
