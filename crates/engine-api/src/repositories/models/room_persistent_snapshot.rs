use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("room_persistent_snapshot", schemaless)]
pub struct RoomPersistentSnapshot {
    #[field(thing)]
    pub room_name: String,
    pub backend: String,
    #[serde(with = "bytes::b64")]
    pub snapshot_bytes: Vec<u8>,
    pub entity_count: i32,
    #[serde(with = "time::iso")]
    pub saved_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("setting", schemaless)]
pub struct Setting {
    #[field(thing)]
    pub name: String,
    pub value: serde_json::Value,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("storage", schemaless)]
#[index(name = "storage_pkey", fields = "collection, key, user_id", unique)]
#[index(
    name = "collection_user_id_read_key_idx",
    fields = "collection, user_id, read, key"
)]
pub struct Storage {
    pub collection: String,
    pub user_id: Uuid,
    pub value: serde_json::Value,
    pub version: String,
    pub read: i16,
    pub write: i16,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("storage_index", schemaless)]
#[index(name = "storage_index_collection_idx", fields = "collection")]
pub struct StorageIndex {
    #[field(thing)]
    pub name: String,
    pub collection: String,
    pub fields: serde_json::Value,
    pub sortable_fields: serde_json::Value,
    pub max_entries: i32,
    pub user_id: Uuid,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("subscription", schemaless)]
#[index(
    name = "subscription_time_user_id_transaction_id_idx",
    fields = "purchase_time, user_id, original_transaction_id"
)]
pub struct Subscription {
    #[field(thing)]
    pub original_transaction_id: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub environment: i16,
    pub product_id: String,
    #[serde(with = "time::iso")]
    pub purchase_time: DateTime<Utc>,
    pub store: i16,
    pub user_id: Uuid,
    #[serde(with = "time::iso")]
    pub expire_time: DateTime<Utc>,
    pub raw_response: serde_json::Value,
    pub raw_notification: serde_json::Value,
    #[serde(with = "time::iso")]
    pub refund_time: DateTime<Utc>,
    pub game_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("user_device", schemaless)]
#[index(name = "user_device_user_id", fields = "user_id")]
pub struct UserDevice {
    #[field(thing)]
    pub id: String,
    pub user_id: Uuid,
    pub preferences: serde_json::Value,
    pub push_token_amazon: String,
    pub push_token_android: String,
    pub push_token_huawei: String,
    pub push_token_ios: String,
    pub push_token_web: String,
    pub provider: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("user_edge", schemaless)]
#[index(
    name = "user_edge_source_id_destination_id_key",
    fields = "source_id, destination_id",
    unique
)]
#[index(name = "user_edge_destination_id", fields = "destination_id")]
#[index(name = "user_edge_source", fields = "source_id, state, position")]
pub struct UserEdge {
    pub source_id: Uuid,
    pub position: i64,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub destination_id: Uuid,
    pub state: i16,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("user_tombstone", schemaless)]
#[index(name = "user_tombstone_user_id_key", fields = "user_id", unique)]
pub struct UserTombstone {
    pub user_id: Uuid,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
