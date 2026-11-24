use super::*;

/// One guest stat increment the engine has already applied.
///
/// The id is the host's `write_id`, so a redelivered intent finds this row and
/// the counter is not raised twice. The row is deliberately internal: it is the
/// idempotency receipt for `player_stat`, not a player-facing history table.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("stat_write", schemaless)]
#[index(name = "stat_write_room_create_time", fields = "room, create_time")]
pub struct StatWriteReceipt {
    #[field(thing)]
    pub id: String,
    pub room: String,
    pub tick: i64,
    pub user_id: Uuid,
    pub stat: String,
    pub delta: i64,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
