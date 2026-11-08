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
    pub(crate) sessions: HashMap<std::rc::Rc<str>, i64>,
    pub(crate) spectators: HashSet<std::rc::Rc<str>>,
    pub(crate) next_player_id: i64,
    pub(crate) empty_since: Option<Instant>,
    pub(crate) born: Instant,
    pub(crate) current_tick: u64,
    pub(crate) rollback_buffer: engine_wasm::RollbackRingBuffer,
    /// Config gate (`[features] voice`): false drops voice packets at the door.
    pub(crate) voice_enabled: bool,
    pub(crate) voice_room: engine_voice::VoiceRoom,
    pub(crate) voice_mesh: engine_voice::VoiceMesh,
    /// Per-session sequence numbers merged into ONE entry (session-slab
    /// step 1), re-keyed from the session String to the player id (step 2):
    /// every caller either already holds the id or resolves it through
    /// `sessions`, so the room stores no per-player key copy at all.
    pub(crate) seqs: HashMap<i64, SessionSeqs>,
    pub(crate) event_seq: u16,
    pub(crate) delta_seq: u16,
    pub(crate) schema_sent: HashSet<std::rc::Rc<str>>,
    pub(crate) pending_leaves: HashMap<std::rc::Rc<str>, (i64, Instant)>,
    pub(crate) patch_skipped: bool,
    pub(crate) last_patch_at: Option<Instant>,
    /// Set once the guest answers `on_patch_due`: the guest then gates its own
    /// delta production (accumulating dirty rows until a due commit), so the
    /// host never needs the skip-then-force-full fallback.
    pub(crate) patch_due_wired: bool,
    /// Optional guest exports resolved once per instance — per-tick
    /// `get_typed_func` name lookups were ~40% of tick CPU in profiles.
    /// Re-probed after `reload_wasm` swaps the instance.
    pub(crate) guest_fns_probed: bool,
    pub(crate) on_patch_rate_fn: Option<wasmtime::TypedFunc<(), i32>>,
    pub(crate) on_simulation_rate_fn: Option<wasmtime::TypedFunc<(), i32>>,
    pub(crate) on_patch_due_fn: Option<wasmtime::TypedFunc<(i32,), ()>>,
    pub(crate) sim_accum: Duration,

    pub(crate) input_journal: VecDeque<JournalInput>,
    /// Input-coalesce staging: newest frame per player id, flushed as one
    /// `on_input` call per player at the next tick. Empty unless
    /// `input_coalesce` is on.
    pub(crate) input_inbox: Vec<(i64, [u8; 20])>,
    /// Config gate (`input_coalesce`): stage inputs for tick-time batch
    /// delivery instead of calling the guest per arriving frame.
    pub(crate) input_coalesce: bool,
    /// Tick-stagger phase slot this room simulates in (assigned round-robin
    /// at creation). Ignored when staggering is off.
    pub(crate) tick_phase: u32,
    pub(crate) state: engine_bus::RoomPhase,
    pub(crate) snapshot_seq: u64,
    pub(crate) result: Option<bytes::Bytes>,
    /// Join options per player id — the JSON the client passed to
    /// `joinOrCreate`. Kept so a hot-swap can re-seat players with the same
    /// data and `onJoin` always sees what the client sent.
    /// Session-slab step 3: options + auth merged into ONE per-player entry.
    pub(crate) entries: HashMap<i64, PlayerEntry>,
    /// Auth data per player id — what `onAuth` returned, handed to `onJoin`
    /// (Colyseus's third argument) and re-applied on hot-swaps.

    /// The merged create options this room was built with (config
    /// `room_options` ⊕ the creator's joinOrCreate options).
    pub(crate) create_options: Vec<u8>,
    /// Per-variant capacity override (from `[rooms.<variant>] max_clients`).
    pub(crate) max_clients_override: Option<i32>,
    /// Per-variant persistence override — a persistent variant's room never
    /// auto-closes for being empty.
    pub(crate) persistent_override: bool,
}

#[derive(Default)]
pub(crate) struct PlayerEntry {
    pub(crate) options: Vec<u8>,
    pub(crate) auth: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(crate) struct SessionSeqs {
    pub(crate) input: u16,
    pub(crate) acked_event: u16,
}

pub(crate) struct JournalInput {
    pub(crate) tick: u64,
    pub(crate) session: std::rc::Rc<str>,
    pub(crate) id: i64,
    pub(crate) seq: u16,
    pub(crate) payload: [u8; 20],
}
