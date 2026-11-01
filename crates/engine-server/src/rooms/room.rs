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
    pub(crate) voice_room: engine_voice::VoiceRoom,
    pub(crate) voice_mesh: engine_voice::VoiceMesh,
    pub(crate) last_input_seq: HashMap<String, u16>,
    pub(crate) event_seq: u16,
    pub(crate) delta_seq: u16,
    pub(crate) schema_sent: HashSet<String>,
    pub(crate) pending_leaves: HashMap<String, (i64, Instant)>,
    pub(crate) patch_skipped: bool,
    pub(crate) last_patch_at: Option<Instant>,
    pub(crate) sim_accum: Duration,
    pub(crate) acked_event_seq: HashMap<String, u16>,
    pub(crate) input_journal: VecDeque<JournalInput>,
    pub(crate) state: engine_bus::RoomPhase,
    pub(crate) snapshot_seq: u64,
    pub(crate) result: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct JournalInput {
    pub(crate) tick: u64,
    pub(crate) session: String,
    pub(crate) id: i64,
    pub(crate) seq: u16,
    pub(crate) payload: [u8; 20],
}
