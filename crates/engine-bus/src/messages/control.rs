use crossbeam_channel::Sender;

use super::PeerId;

#[derive(Clone, Debug)]
pub struct MatchFinished {
    pub room: String,

    pub game_id: String,

    pub end_reason: String,

    pub started_at_unix: i64,
    pub ended_at_unix: i64,
    pub ticks: i64,

    pub peak_players: i64,

    pub snapshot_seq: i64,

    pub result: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct RoomSnapshot {
    pub room: String,

    pub backend: String,

    pub seq: i64,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct SnapshotFetch {
    pub room: String,
    pub reply: Sender<Option<RoomSnapshot>>,
}

#[derive(Clone, Debug)]
pub struct RoomOwnership {
    pub room: String,

    pub node: String,

    pub game: String,

    pub state: crate::telemetry::RoomPhase,

    pub peers: i64,
    pub tick: i64,

    pub snapshot_seq: i64,
}

/// One stat increment a room's guest asked the engine to record.
///
/// The room only names the seat and the counter. `write_id` is the host's
/// idempotency key: the control plane stores it with the row it applied, so a
/// redelivered write can never double-count.
#[derive(Clone, Debug)]
pub struct StatWrite {
    pub room: String,
    pub write_id: String,
    pub tick: i64,
    pub seat: i64,
    pub stat: String,
    pub delta: i64,
}

#[derive(Clone, Debug)]
pub struct SeatUpdate {
    pub room: String,
    pub seat: i64,

    pub user_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadKind {
    Upload,
    Rollback,
}

#[derive(Clone, Debug)]
pub struct WasmReload {
    pub game_id: String,

    pub game_key: String,

    pub version: i32,

    pub sha256: String,

    pub bytes: Vec<u8>,
    pub kind: ReloadKind,
}

#[derive(Clone, Debug)]
pub struct MapReload {
    pub game_key: String,

    pub version: i32,
    pub format: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct SettingsReload {
    pub tick_rate: u32,
}

#[derive(Debug)]
pub enum SimCommand {
    BindPeer {
        peer_id: PeerId,
        room: String,
        reply: Sender<i64>,
    },
    DropPeer {
        peer_id: PeerId,
    },

    Event {
        room: String,
        target: Option<PeerId>,
        bytes: Vec<u8>,
    },
}
