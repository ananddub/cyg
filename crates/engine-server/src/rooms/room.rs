use std::collections::VecDeque;

/// Room maps are keyed by short session ids and player ids and are touched on
/// every input frame and every tick, so the hasher matters: `std`'s SipHash is
/// DoS-resistant but slow, and nothing here is attacker-controlled in a way
/// that matters (keys are opaque ids we generated). FxHash is ~2-3x faster on
/// these short keys.
pub(crate) type HashMap<K, V> = std::collections::HashMap<K, V, rustc_hash::FxBuildHasher>;
pub(crate) type HashSet<K> = std::collections::HashSet<K, rustc_hash::FxBuildHasher>;

/// Pre-sizes a per-room map for the room's client cap. These maps grow one
/// join at a time and are touched on every input frame and tick; starting at
/// full capacity removes the incremental rehash churn from the join path that
/// shows up as tail spikes at 4000+ CCU. The hint is a floor, not a limit:
/// oversized or unbounded variants just keep growing as before.
pub(crate) fn session_map<V>(max_clients: i32) -> HashMap<std::rc::Rc<str>, V> {
    HashMap::with_capacity_and_hasher(
        max_clients.max(0) as usize,
        rustc_hash::FxBuildHasher::default(),
    )
}

/// Same hint for the per-player-id maps (`seqs`, `entries`).
pub(crate) fn player_map<V>(max_clients: i32) -> HashMap<i64, V> {
    HashMap::with_capacity_and_hasher(
        max_clients.max(0) as usize,
        rustc_hash::FxBuildHasher::default(),
    )
}
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
    /// Per-player state view. Absent = every field visible (subject to
    /// `.private()`). The mask is resolved from guest intents and applied to
    /// full and patch frames before they reach the wire.
    pub(crate) view_masks: HashMap<i64, crate::netcode::ViewMask>,
    /// Players whose view changed and who still need a fresh full snapshot.
    /// Kept separate from `schema_sent`: a late `schema_ready` ack could
    /// otherwise re-mark the seat as "full already sent" before the mask took
    /// effect, leaking the old field value for one patch.
    pub(crate) view_dirty: HashSet<i64>,
    pub(crate) event_seq: u16,
    pub(crate) delta_seq: u16,
    pub(crate) schema_sent: HashSet<std::rc::Rc<str>>,
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
    pub(crate) input_inbox: Vec<(i64, Vec<u8>)>,
    /// Config gate (`input_coalesce`): stage inputs for tick-time batch
    /// delivery instead of calling the guest per arriving frame.
    pub(crate) input_coalesce: bool,
    /// Per-field input domains (Colyseus `sanitize` map form) applied IN
    /// PLACE to every 20-byte frame before any consumer — guest, journal,
    /// rollback replay — reads it. Starts at the standard frame's defaults;
    /// the room's `Input` schema overrides land via `set_input_domains`.
    pub(crate) input_domains: engine_schema::InputDomainMap,
    /// Hold-last idle synthesis: per-player id, the LAST guest-ready frame
    /// the seat sent (20-byte mesh payload or the schema lane's app frame).
    /// Empty-tick sessions get this frame re-delivered while connected (see
    /// `synthesize_idle_inputs`).
    pub(crate) last_input: HashMap<i64, Vec<u8>>,
    /// Player ids that sent at least one input since the last tick — keeps
    /// idle synthesis from double-delivering a frame the tick already got.
    pub(crate) inputs_since_tick: HashSet<i64>,
    /// Room-configurable idle policy: HOLD-LAST gated on connection liveness
    /// (default), or plain SKIP (`false`) — the strict fixed-timestep
    /// behaviour where an empty tick simply runs no input.
    pub(crate) idle_input_hold: bool,
    /// Tick-stagger phase slot this room simulates in (assigned round-robin
    /// at creation). Ignored when staggering is off.
    pub(crate) tick_phase: u32,
    pub(crate) lifecycle: engine_manager::RoomLifecycle,
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
    /// Last client RTT stamp echoed back on the delta tail (Phase D RTT
    /// echo). 0 = never stamped (old clients).
    pub(crate) stamp: u16,
}

pub(crate) struct JournalInput {
    pub(crate) tick: u64,
    pub(crate) session: std::rc::Rc<str>,
    pub(crate) id: i64,
    pub(crate) seq: u16,
    pub(crate) payload: [u8; 20],
}
