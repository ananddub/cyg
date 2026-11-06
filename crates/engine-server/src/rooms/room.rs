use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use engine_nav::nav2d::NavMesh2D;

use super::{OutboundMessage, RoomBackend, RoomHostData, RoomLogic};

pub struct GameRoom {
    pub(crate) backend: RoomBackend,
    pub(crate) storage_enabled: bool,
    pub(crate) wasm: Option<RoomLogic<RoomHostData>>,
    pub(crate) arena_2d: Option<engine_physics::MapConfig2D>,
    pub(crate) navmesh: Option<std::sync::Arc<NavMesh2D>>,
    pub(crate) ecs: Option<crate::ecs_room::EcsRoomState>,
    pub(crate) ecs_outbox: Vec<OutboundMessage>,
    pub(crate) sessions: HashMap<String, i64>,
    pub(crate) spectators: HashSet<String>,
    pub(crate) next_player_id: i64,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) born: Instant,
    pub(crate) current_tick: u64,
    pub(crate) rollback_buffer: engine_wasm::RollbackRingBuffer,
    /// Config gate (`[features] voice`): false drops voice packets at the door.
    pub(crate) voice_enabled: bool,
    pub(crate) voice_room: engine_voice::VoiceRoom,
    pub(crate) voice_mesh: engine_voice::VoiceMesh,
    pub(crate) last_input_seq: HashMap<String, u16>,
    pub(crate) event_seq: u16,
    pub(crate) delta_seq: u16,
    pub(crate) schema_sent: HashSet<String>,
    pub(crate) pending_leaves: HashMap<String, (i64, Instant)>,
    pub(crate) patch_skipped: bool,
    pub(crate) last_patch_at: Option<Instant>,
    /// Set once the guest answers `on_patch_due`: the guest then gates its own
    /// delta production (accumulating dirty rows until a due commit), so the
    /// host never needs the skip-then-force-full fallback.
    pub(crate) patch_due_wired: bool,
    pub(crate) sim_accum: Duration,
    pub(crate) acked_event_seq: HashMap<String, u16>,
    pub(crate) input_journal: VecDeque<JournalInput>,
    pub(crate) state: engine_bus::RoomPhase,
    pub(crate) snapshot_seq: u64,
    pub(crate) result: Option<Vec<u8>>,
    /// Join options per player id — the JSON the client passed to
    /// `joinOrCreate`. Kept so a hot-swap can re-seat players with the same
    /// data and `onJoin` always sees what the client sent.
    pub(crate) player_options: HashMap<i64, Vec<u8>>,
    /// Auth data per player id — what `onAuth` returned, handed to `onJoin`
    /// (Colyseus's third argument) and re-applied on hot-swaps.
    pub(crate) auth_data: HashMap<i64, Vec<u8>>,
    /// The merged create options this room was built with (config
    /// `room_options` ⊕ the creator's joinOrCreate options).
    pub(crate) create_options: Vec<u8>,
    /// Per-variant capacity override (from `[rooms.<variant>] max_clients`).
    pub(crate) max_clients_override: Option<i32>,
    /// Per-variant persistence override — a persistent variant's room never
    /// auto-closes for being empty.
    pub(crate) persistent_override: bool,
}

#[derive(Clone)]
pub(crate) struct JournalInput {
    pub(crate) tick: u64,
    pub(crate) session: String,
    pub(crate) id: i64,
    pub(crate) seq: u16,
    pub(crate) payload: [u8; 20],
}
