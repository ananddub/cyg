use super::*;

/// One match the host finished: how long it ran, how many it held, and why it
/// ended. Written from the bus when a room is disposed, so a player can ask what
/// happened after the process that ran it is gone.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("match_history", schemaless)]
#[index(name = "match_history_room_ended_at_idx", fields = "room, ended_at")]
#[index(name = "match_history_game_id", fields = "game_id")]
pub struct MatchHistory {
    #[field(thing)]
    pub id: Uuid,
    pub game_id: String,
    pub room: String,
    /// `empty`, `duration` or `drain` — the host's own close reasons.
    pub end_reason: String,
    #[serde(with = "time::iso")]
    pub started_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub ended_at: DateTime<Utc>,
    pub ticks: i64,
    pub peak_players: i64,
    /// Which snapshot of the room this match ended on: the ordinal of the last
    /// state the host saved. `0` means the room never persisted one, so there is
    /// nothing to replay from even if the event rows are there.
    pub snapshot_seq: i64,
    /// The guest's own outcome, byte for byte as `host_send` put it on the wire
    /// (msgpack, event name `[gameplay] result_event`). Empty when the game
    /// never declared one — the engine does not invent a score it was not told.
    #[serde(with = "bytes::b64")]
    pub result: Vec<u8>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
