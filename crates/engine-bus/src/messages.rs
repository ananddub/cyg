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

/// Why the sim should hot-swap its room module: a console upload of a fresh
/// version, or a rollback re-activating an older one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadKind {
    Upload,
    Rollback,
}

/// Control plane → sim: the active WASM module for one game changed and live
/// rooms should be swapped onto it (Level 1 hot-reload). Identity only — the
/// bytes stay in `room_modules`; the reader fetches the version it is told to
/// load (engine-api `WasmService::get_module_bytes`).
#[derive(Clone, Debug)]
pub struct WasmReload {
    /// `games.id` as text; the bus carries no sqlx types.
    pub game_id: String,
    /// `games.game_key`, so a sim can match the reload against its own game
    /// without a second lookup.
    pub game_key: String,
    /// The newly active `room_modules.version`.
    pub version: i32,
    /// SHA-256 hex of that version's binary — the integrity anchor the sim can
    /// verify whatever it loads against.
    pub sha256: String,
    pub kind: ReloadKind,
}

/// Control plane → sim: the active MAP for one game changed. Unlike wasm, the
/// bytes ride along — a map is small, applied only at room spawn (new rooms
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
