use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("console_acl_template", schemaless)]
#[index(name = "template_name_uniq", fields = "name", unique)]
pub struct ConsoleAclTemplate {
    #[field(thing)]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub acl: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("console_audit_log", schemaless)]
#[index(name = "console_audit_log_console_user", fields = "console_user_id")]
pub struct ConsoleAuditLog {
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[field(thing)]
    pub id: Uuid,
    pub console_user_id: Uuid,
    pub console_username: String,
    pub email: String,
    pub action: String,
    pub resource: String,
    pub message: String,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("console_user", schemaless)]
#[index(name = "console_user_email_uniq", fields = "email", unique)]
#[index(name = "console_user_username_uniq", fields = "username", unique)]
pub struct ConsoleUser {
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub disable_time: DateTime<Utc>,
    pub email: String,
    #[field(thing)]
    pub id: Uuid,
    pub metadata: serde_json::Value,
    #[serde(with = "bytes::b64_opt")]
    pub password: Option<Vec<u8>>,
    pub role: i16,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub username: String,
    #[serde(with = "bytes::b64_opt")]
    pub mfa_secret: Option<Vec<u8>>,
    #[serde(with = "bytes::b64_opt")]
    pub mfa_recovery_codes: Option<Vec<u8>>,
    pub mfa_required: Option<bool>,
    pub acl: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("game_maps", schemaless)]
#[index(name = "game_maps_game_version", fields = "game_id, version", unique)]
#[index(name = "idx_game_maps_game", fields = "game_id, version")]
pub struct GameMaps {
    #[field(thing)]
    pub id: Uuid,
    pub game_id: Uuid,
    pub version: i32,
    pub name: String,
    pub format: String,
    pub blob_key: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub is_active: bool,
    #[serde(with = "time::iso")]
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("games", schemaless)]
#[index(name = "games_game_key_key", fields = "game_key", unique)]
pub struct Games {
    #[field(thing)]
    pub id: Uuid,
    pub game_key: String,
    pub name: String,
    pub api_key: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub status: String,
    pub config: serde_json::Value,
    #[serde(with = "time::iso")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("group_edge", schemaless)]
#[index(
    name = "group_edge_source_id_destination_id_key",
    fields = "source_id, destination_id",
    unique
)]
#[index(name = "group_edge_source", fields = "source_id, state, position")]
pub struct GroupEdge {
    pub source_id: Uuid,
    pub position: i64,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub destination_id: Uuid,
    pub state: i16,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("groups", schemaless)]
#[index(name = "groups_name_key", fields = "name", unique)]
#[index(name = "groups_update_time", fields = "update_time, id")]
pub struct Groups {
    #[field(thing)]
    pub id: Uuid,
    pub creator_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub state: i16,
    pub edge_count: i32,
    pub max_count: i32,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub disable_time: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("item_definition", schemaless)]
pub struct ItemDefinition {
    #[field(thing)]
    pub item_id: String,
    pub name: String,
    pub category: String,
    pub max_stack: i32,
    pub rarity: String,
    pub properties: serde_json::Value,
    #[serde(with = "time::iso")]
    pub created_at: DateTime<Utc>,
}
