use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("player_inventory_item", schemaless)]
#[index(name = "idx_player_inventory_item_user", fields = "user_id")]
pub struct PlayerInventoryItem {
    #[field(thing)]
    pub instance_id: Uuid,
    pub user_id: Uuid,
    pub item_id: String,
    pub quantity: i32,
    pub slot_index: i32,
    pub metadata: serde_json::Value,
    #[serde(with = "time::iso")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("leaderboard", schemaless)]
#[index(name = "idx_leaderboard_game", fields = "game_id")]
#[index(name = "leaderboard_create_time_id_idx", fields = "create_time, id")]
pub struct Leaderboard {
    #[field(thing)]
    pub id: String,
    pub authoritative: bool,
    pub sort_order: i16,
    pub operator: i16,
    pub reset_schedule: Option<String>,
    pub metadata: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    pub category: i16,
    pub description: String,
    pub duration: i32,
    #[serde(with = "time::iso")]
    pub end_time: DateTime<Utc>,
    pub join_required: bool,
    pub max_size: i32,
    pub max_num_score: i32,
    pub title: String,
    pub size: i32,
    #[serde(with = "time::iso")]
    pub start_time: DateTime<Utc>,
    pub enable_ranks: Option<bool>,
    pub game_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("leaderboard_record", schemaless)]
#[index(
    name = "leaderboard_record_pkey",
    fields = "owner_id, leaderboard_id, expiry_time",
    unique
)]
#[index(
    name = "owner_id_expiry_time_leaderboard_id_idx",
    fields = "owner_id, expiry_time, leaderboard_id"
)]
pub struct LeaderboardRecord {
    pub leaderboard_id: String,
    pub owner_id: Uuid,
    pub username: Option<String>,
    pub score: i64,
    pub subscore: i64,
    pub num_score: i32,
    pub metadata: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub expiry_time: DateTime<Utc>,
    pub max_num_score: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("message", schemaless)]
#[index(name = "message_sender_id", fields = "sender_id")]
#[index(name = "message_stream", fields = "stream_subject, create_time")]
pub struct Message {
    #[field(thing)]
    pub id: Uuid,
    pub code: i16,
    pub sender_id: Uuid,
    pub username: String,
    pub stream_mode: i16,
    pub stream_subject: Uuid,
    pub stream_descriptor: Uuid,
    pub stream_label: String,
    pub content: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("notification", schemaless)]
#[index(name = "notification_user_id", fields = "user_id")]
pub struct Notification {
    #[field(thing)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub subject: String,
    pub content: serde_json::Value,
    pub code: i16,
    pub sender_id: Uuid,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("purchase", schemaless)]
#[index(
    name = "purchase_time_user_id_transaction_id_idx",
    fields = "purchase_time, user_id, transaction_id"
)]
pub struct Purchase {
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    pub environment: i16,
    pub product_id: String,
    #[serde(with = "time::iso")]
    pub purchase_time: DateTime<Utc>,
    pub raw_response: serde_json::Value,
    pub store: i16,
    #[field(thing)]
    pub transaction_id: String,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub user_id: Uuid,
    #[serde(with = "time::iso")]
    pub refund_time: DateTime<Utc>,
    pub game_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("room_modules", schemaless)]
#[index(name = "room_modules_game_version", fields = "game_id, version", unique)]
#[index(name = "idx_room_modules_game", fields = "game_id, version")]
pub struct RoomModules {
    #[field(thing)]
    pub id: Uuid,
    pub game_id: Uuid,
    pub version: i32,
    pub filename: String,
    pub blob_key: Option<String>,
    pub size_bytes: i64,
    pub source_code: Option<String>,
    pub sha256: String,
    pub is_active: bool,
    #[serde(with = "time::iso")]
    pub created_at: DateTime<Utc>,
}
