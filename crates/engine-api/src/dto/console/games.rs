use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const ENGINE_TYPES: [&str; 5] = ["unity", "unreal", "godot", "web", "custom"];

pub const GAME_BACKEND_MODES: [&str; 2] = ["relay", "wasm"];

pub const GAME_STATUSES: [&str; 3] = ["active", "suspended", "archived"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameConfig {
    pub tick_rate: u32,
    pub max_clients: u32,
    pub persistent_rooms: bool,
    pub auto_close_seconds: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            tick_rate: 128,
            max_clients: 16,
            persistent_rooms: false,
            auto_close_seconds: 30,
        }
    }
}

impl GameConfig {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateGameRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub tick_rate: Option<u32>,
    #[serde(default)]
    pub max_clients: Option<u32>,
    #[serde(default)]
    pub persistent_rooms: Option<bool>,
    #[serde(default)]
    pub auto_close_seconds: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateGameRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub tick_rate: Option<u32>,
    #[serde(default)]
    pub max_clients: Option<u32>,
    #[serde(default)]
    pub persistent_rooms: Option<bool>,
    #[serde(default)]
    pub auto_close_seconds: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct GameRow {
    pub id: Uuid,
    pub game_key: String,
    pub name: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub status: String,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GameDto {
    pub id: Uuid,
    pub game_key: String,
    pub name: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub status: String,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<GameRow> for GameDto {
    fn from(row: GameRow) -> Self {
        Self {
            id: row.id,
            game_key: row.game_key,
            name: row.name,
            client_key: row.client_key,
            engine_type: row.engine_type,
            backend_mode: row.backend_mode,
            status: row.status,
            config: row.config,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GameDetailDto {
    pub id: Uuid,
    pub game_key: String,
    pub name: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub status: String,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub api_key: String,
    pub active_module_version: Option<i32>,
    pub module_count: i64,
}

#[derive(Debug, Serialize)]
pub struct GameListDto {
    pub games: Vec<GameDto>,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct ArchiveGameResponse {
    pub archived: bool,
}

#[derive(Debug, Serialize)]
pub struct UnityConnectionDto {
    #[serde(rename = "gameKey")]
    pub game_key: String,
    #[serde(rename = "clientKey")]
    pub client_key: String,
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(rename = "serverHttpUrl")]
    pub server_http_url: String,
    #[serde(rename = "serverWsPath")]
    pub server_ws_path: String,
    #[serde(rename = "defaultRoom")]
    pub default_room: String,
    #[serde(rename = "tickRate")]
    pub tick_rate: u32,
    #[serde(rename = "maxPlayers")]
    pub max_players: u32,
    #[serde(rename = "persistentRooms")]
    pub persistent_rooms: bool,
    #[serde(rename = "backendMode")]
    pub backend_mode: String,
}
