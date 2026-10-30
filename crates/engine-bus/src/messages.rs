use crossbeam_channel::Sender;

pub type PeerId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    NativeUdp,
    Browser,
}

/// `Unreliable` is newest-wins and droppable; `Reliable` is never dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Unreliable,
    Reliable,
}

/// One client input traveling planes → sim.
#[derive(Clone, Debug)]
pub struct InputMsg {
    pub player_id: i64,
    /// Client input sequence, echoed as `acked_seq` in the delta tail.
    pub seq: u16,
    /// Handed verbatim to `wasm.on_input`; planes never interpret it.
    pub payload: [u8; 20],
    /// `mono_ns()` at kernel read time; the sim subtracts it on drain.
    pub arrival_ns: u64,
    pub via: Via,
}

/// `bytes` is exactly the on-the-wire frame built by the sim's channel layer.
#[derive(Clone, Debug)]
pub struct OutFrame {
    pub peer_id: PeerId,
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

/// sim → control plane: one gameplay event the guest emitted through
/// `host_send`, mirrored off the outbound wire so it can be persisted and
/// replayed later. The same message the clients see, stamped with where it came
/// from; nothing about it is game-specific, which is why it rides the bus
/// instead of a per-genre hook.
#[derive(Clone, Debug)]
pub struct SimEvent {
    pub room: String,
    /// Room tick the event was emitted on, so a reader can line events up with
    /// a replay.
    pub tick: i64,
    /// Mirror ordinal, unique and increasing for the life of the process, so a
    /// reader can spot a gap left by a full queue. The game-meaningful position
    /// is `(room, tick)`.
    pub seq: u64,
    /// Addressee player id; `0` is room-wide.
    pub target: i64,
    pub event: String,
    pub payload: Vec<u8>,
}

/// One match the host just disposed: how long it ran, how many people it held,
/// and why it ended. The host has no database handle, so this rides the bus and
/// the control plane turns it into a `match_history` row.
#[derive(Clone, Debug)]
pub struct MatchFinished {
    pub room: String,
    /// The game this process serves, so history stays readable after the room
    /// name is reused by a later match.
    pub game_id: String,
    /// `empty` | `duration` | `drain` — the host's own close reasons.
    pub end_reason: String,
    /// Wall seconds, because `Instant` means nothing across a process restart.
    pub started_at_unix: i64,
    pub ended_at_unix: i64,
    pub ticks: i64,
    /// Most seats held at once, sampled once per host tick.
    pub peak_players: i64,
    /// Ordinal of the last snapshot this room exported, so the match row names
    /// the state generation a replay starts from. `0` when the room never
    /// persisted anything (no `storage` capability, or a relay room).
    pub snapshot_seq: i64,
    /// The guest's own outcome: the payload of the last `host_send` whose event
    /// name matched `[gameplay] result_event`. Raw wire bytes — msgpack, the
    /// engine never decodes it — and empty when the game declared no result.
    pub result: Vec<u8>,
}

/// A room's saved state, handed over where the host would otherwise have kept it
/// in its own process memory. The control plane writes it to
/// `room_persistent_snapshot`, so a snapshot survives the node that took it and
/// a replay can start from real state instead of a guess.
#[derive(Clone, Debug)]
pub struct RoomSnapshot {
    pub room: String,
    /// Which backend produced the bytes: the format is backend-specific, and a
    /// reader cannot tell `wasm` from `ecs` by looking.
    pub backend: String,
    /// Which export of this room's state these bytes are, matching the
    /// `snapshot_seq` the match row points at.
    pub seq: i64,
    pub bytes: Vec<u8>,
}

/// sim → control plane: the host is about to spawn a room it has no copy of in
/// this process and asks for the state a previous run saved. The reply carries
/// the stored row, or `None` when the room never persisted anything — which is
/// also what a control plane that is not pumping leaves the host with, after
/// [`Bus::fetch_snapshot`](crate::Bus::fetch_snapshot) gives up waiting.
#[derive(Debug)]
pub struct SnapshotFetch {
    pub room: String,
    pub reply: Sender<Option<RoomSnapshot>>,
}

/// Why the sim should hot-swap its room module: a console upload of a fresh
/// version, or a rollback re-activating an older one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadKind {
    Upload,
    Rollback,
}

/// Control plane → sim: the active WASM module for one game changed and live
/// rooms should be swapped onto it (Level 1 hot-reload). The bytes ride along
/// like [`MapReload`]: the sim thread has no database handle, and a reload is
/// rare enough that one module on the queue is cheaper than an async fetch
/// bridge out of the tick loop.
#[derive(Clone, Debug)]
pub struct WasmReload {
    /// `games.id` as text; the bus carries no database-handle types.
    pub game_id: String,
    /// `games.game_key`, so a sim can match the reload against its own game
    /// without a second lookup.
    pub game_key: String,
    /// The newly active `room_modules.version`.
    pub version: i32,
    /// SHA-256 hex of that version's binary — the integrity anchor the reader
    /// checks the payload against before swapping.
    pub sha256: String,
    /// The version's `.wasm` bytes, exactly as the blob store holds them.
    pub bytes: Vec<u8>,
    pub kind: ReloadKind,
}

/// Control plane → sim: the active MAP for one game changed. Like [`WasmReload`]
/// the bytes ride along — a map is small, applied only at room spawn (new rooms
/// clone the host's arena), and making the reader re-fetch over SQL buys
/// nothing. Formats mirror `game_maps.format` (`oxcl`, `map2d`, `map3d`,
/// `navmesh`).
#[derive(Clone, Debug)]
pub struct MapReload {
    pub game_key: String,
    /// The newly active `game_maps.version`.
    pub version: i32,
    pub format: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Control plane → sim: a runtime knob changed in the console. Only the knobs
/// the host loop can adopt mid-flight ride here — today the tick rate; boot-only
/// fields (bind ports, module paths) stay config.toml's business.
#[derive(Clone, Copy, Debug)]
pub struct SettingsReload {
    /// The new tick rate in Hz; the host recomputes its tick interval.
    pub tick_rate: u32,
}

/// Everything the sim accepts from the other planes.
#[derive(Debug)]
pub enum SimCommand {
    /// The sim binds the peer, assigns the player id and answers `reply`.
    BindPeer {
        peer_id: PeerId,
        room: String,
        reply: Sender<i64>,
    },
    DropPeer {
        peer_id: PeerId,
    },
    /// `target: None` broadcasts to the room.
    Event {
        room: String,
        target: Option<PeerId>,
        bytes: Vec<u8>,
    },
}
