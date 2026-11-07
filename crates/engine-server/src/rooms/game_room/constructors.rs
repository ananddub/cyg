use crate::rooms::*;
use engine_nav::nav2d::NavMesh2D;
use engine_physics::PhysicsWorld3D;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

impl GameRoom {

    pub(crate) fn new(
        pre: &engine_wasm::WasmPreInstance<RoomHostData>,
        max_clients: i32,
        physics: PhysicsWorld3D,
    ) -> Result<Self, engine_wasm::WasmError> {
        Self::new_with_extras(pre, max_clients, physics, None, None, b"", &Default::default())
    }

    pub(crate) fn new_with_extras(
        pre: &engine_wasm::WasmPreInstance<RoomHostData>,
        max_clients: i32,
        physics: PhysicsWorld3D,
        arena_2d: Option<&engine_physics::MapConfig2D>,
        navmesh: Option<std::sync::Arc<NavMesh2D>>,
        create_options: &[u8],
        features: &engine_config::FeaturesConfig,
    ) -> Result<Self, engine_wasm::WasmError> {
        let mut host_data = RoomHostData::new_with_features(physics, features);
        if let Some(map) = arena_2d {
            host_data.load_arena_2d(map);
        }
        host_data.navmesh = navmesh.clone();
        let mut wasm = RoomLogic::from_pre(pre, host_data)?;
        GameRoom::call_on_create(&mut wasm, max_clients, create_options);
        Ok(Self {
            backend: RoomBackend::Wasm,
            arena_2d: arena_2d.cloned(),
            navmesh,
            storage_enabled: true,
            voice_enabled: features.voice,
            wasm: Some(wasm),
            ecs: None,
            ecs_outbox: Vec::new(),
            sessions: HashMap::new(),
            spectators: HashSet::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
            born: Instant::now(),
            current_tick: 0,
            rollback_buffer: engine_wasm::RollbackRingBuffer::default(),
            voice_room: engine_voice::VoiceRoom::new(engine_voice::DistanceModel::Inverse {
                min_distance: 10.0,
                max_distance: 250.0,
                rolloff: 1.0,
            }),
            voice_mesh: engine_voice::VoiceMesh::default(),
            seqs: HashMap::new(),
            event_seq: 0,
            delta_seq: 0,
            schema_sent: HashSet::new(),
            pending_leaves: HashMap::new(),
            patch_skipped: false,
            patch_due_wired: false,
            last_patch_at: None,
            sim_accum: Duration::ZERO,
            input_journal: VecDeque::new(),
            input_inbox: Vec::new(),
            input_coalesce: false,
            state: engine_bus::RoomPhase::Created,
            snapshot_seq: 0,
            result: None,
            entries: HashMap::new(),
            create_options: create_options.to_vec(),
            max_clients_override: None,
            persistent_override: false,
        })
    }

    pub(crate) fn new_relay() -> Self {
        Self {
            backend: RoomBackend::Relay,
            arena_2d: None,
            navmesh: None,
            storage_enabled: true,
            voice_enabled: true,
            wasm: None,
            ecs: None,
            ecs_outbox: Vec::new(),
            sessions: HashMap::new(),
            spectators: HashSet::new(),
            next_player_id: 1,
            empty_since: Some(Instant::now()),
            born: Instant::now(),
            current_tick: 0,
            rollback_buffer: engine_wasm::RollbackRingBuffer::default(),
            voice_room: engine_voice::VoiceRoom::new(engine_voice::DistanceModel::Inverse {
                min_distance: 10.0,
                max_distance: 250.0,
                rolloff: 1.0,
            }),
            voice_mesh: engine_voice::VoiceMesh::default(),
            seqs: HashMap::new(),
            event_seq: 0,
            delta_seq: 0,
            schema_sent: HashSet::new(),
            pending_leaves: HashMap::new(),
            patch_skipped: false,
            patch_due_wired: false,
            last_patch_at: None,
            sim_accum: Duration::ZERO,
            input_journal: VecDeque::new(),
            input_inbox: Vec::new(),
            input_coalesce: false,
            state: engine_bus::RoomPhase::Created,
            snapshot_seq: 0,
            result: None,
            entries: HashMap::new(),
            create_options: Vec::new(),
            max_clients_override: None,
            persistent_override: false,
        }
    }

    pub(crate) fn new_ecs(name: &str, arena: Option<&engine_physics::oxcl::CookedMesh>) -> Self {
        let mut room = Self::new_relay();
        room.backend = RoomBackend::Ecs;
        room.ecs = Some(crate::ecs_room::EcsRoomState::new(name, arena));
        room
    }

}
