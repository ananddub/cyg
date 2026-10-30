use super::*;

/// Who runs a room, right now. One row per live room, refreshed by the host that
/// owns it and released when it disposes the room, so a second node can tell a
/// room that moved from one that died: the row's `update_time` is a lease, and a
/// stale one is nobody's.
///
/// Keyed by the room name rather than an id, because that is what a seat token,
/// a `SimCommand::BindPeer` and a saved snapshot are all keyed by — a second
/// identifier here would only give the registry a way to disagree with them.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("room_registry", schemaless)]
#[index(name = "room_registry_node_id_idx", fields = "node_id")]
pub struct RoomRegistry {
    #[field(thing)]
    pub room_id: String,
    pub node_id: String,
    pub game_id: String,
    /// The host's own lifecycle phase for the room: `created`, `active`,
    /// `hibernating`. A released name has no row, so `closed` never reaches the
    /// table.
    pub state: String,
    pub peers: i64,
    pub tick: i64,
    /// The room's save counter as the owner last saw it: which snapshot a
    /// replacement node should expect to find, and how many were lost with it.
    pub snapshot_seq: i64,
    /// Written when the claim arrived, not when the host took it: a pump that
    /// stopped is exactly the gap a reader needs to see.
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
