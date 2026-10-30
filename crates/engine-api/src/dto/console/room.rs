use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::repositories::models::RoomRegistry;
use crate::repositories::room_registry::lease_lapsed;

/// One room the registry names an owner for. Cross-node by construction: every
/// other room surface this console has is the in-process host's, so this is the
/// only place an operator can see a room running on somebody else's pod.
#[derive(Debug, Serialize)]
pub struct ConsoleRoomRow {
    pub room_id: String,
    pub node_id: String,
    pub game_id: String,
    pub state: String,
    pub peers: i64,
    pub tick: i64,
    pub snapshot_seq: i64,
    pub update_time: DateTime<Utc>,
    /// The owner stopped refreshing this row. The room is not necessarily gone —
    /// a host that is alive re-claims every second — so this is the read that
    /// tells a dying node from a dead one before anything is re-created.
    pub stale: bool,
}

impl ConsoleRoomRow {
    pub fn from_registry(row: RoomRegistry) -> ConsoleRoomRow {
        let stale = lease_lapsed(&row, Utc::now());
        ConsoleRoomRow {
            room_id: row.room_id,
            node_id: row.node_id,
            game_id: row.game_id,
            state: row.state,
            peers: row.peers,
            tick: row.tick,
            snapshot_seq: row.snapshot_seq,
            update_time: row.update_time,
            stale,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConsoleRoomList {
    pub rooms: Vec<ConsoleRoomRow>,
}
