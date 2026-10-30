use super::*;

/// One guest `host_send` message the game listed in `[gameplay] record_events`,
/// mirrored off the outbound wire by the host. A row exists so a finished match
/// can be replayed and an operator can read what the game claimed happened,
/// without a client's word for it.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("gameplay_event", schemaless)]
#[index(
    name = "gameplay_event_room_create_time_idx",
    fields = "room, create_time"
)]
#[index(name = "gameplay_event_create_time", fields = "create_time")]
pub struct GameplayEvent {
    #[field(thing)]
    pub id: Uuid,
    pub room: String,
    /// Room tick the message was emitted on, so rows line up with a replay.
    pub tick: i64,
    /// Mirror ordinal, increasing for the life of the host process: a gap is a
    /// message the bounded queue could not deliver, not a game that skipped it.
    pub seq: i64,
    /// Addressee player id; 0 is room-wide.
    pub player_id: i64,
    pub event: String,
    #[serde(with = "bytes::b64")]
    pub payload: Vec<u8>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}

/// A tick where a client's checksum report disagreed with the host checkpoint.
/// Split out from [`GameplayEvent`] because it is the engine's own evidence:
/// every game produces it whether or not it listed any events, and an operator
/// reads it alone, in order, to find where prediction diverged.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("desync_report", schemaless)]
#[index(
    name = "desync_report_room_create_time_idx",
    fields = "room, create_time"
)]
#[index(name = "desync_report_create_time", fields = "create_time")]
pub struct DesyncReport {
    #[field(thing)]
    pub id: Uuid,
    pub room: String,
    /// Reporting player id; 0 on the raw-UDP lane, which has no session map.
    pub player_id: i64,
    pub tick: i64,
    /// Both sides as 16-char lowercase hex — a `u64` bit pattern does not fit an
    /// `i64`, and the operator reads these as hex anyway.
    pub expected_checksum: String,
    pub client_checksum: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
