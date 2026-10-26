//! Games (`games`) as the console lists and writes them, and the per-game runtime
//! configuration stored in `games.config`.
//!
//! `api_key` is deliberately absent from every shape except [`GameDetailDto`]: the
//! listing and the write replies carry the game's identity but never its secret, so a
//! table can be browsed without the credential leaking into a UI. `client_key` is the
//! opposite case — a publishable credential the built client ships with — so it rides
//! on every shape, list included.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The engines a game may register against. Anything else is refused: the WASM
/// module plane and the seat issuer both branch on this value. `custom` is a
/// hand-rolled server-authoritative engine, treated like `web` for the backend mode.
pub const ENGINE_TYPES: [&str; 5] = ["unity", "unreal", "godot", "web", "custom"];

/// The two backend modes the product ships: `relay` is the pure C# relay + BaaS plane
/// (no WASM), `wasm` is the authoritative server simulation an uploaded room module
/// runs. The mode is derived from the engine, never chosen by the caller.
pub const GAME_BACKEND_MODES: [&str; 2] = ["relay", "wasm"];

/// The lifecycles a game may hold. `archived` is what a delete writes — the row and
/// its modules stay, so an operator can read the history of a game that is gone from
/// the dashboard.
pub const GAME_STATUSES: [&str; 3] = ["active", "suspended", "archived"];

/// Per-game runtime tuning, the document `games.config` holds. The defaults are the
/// column default in `db/schema/games.sql`, restated here so a partial config still
/// reads as a complete one.
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
    /// The document as the column stores it, so a create that named no config and a
    /// create that named the defaults write byte-identical rows.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

/// `POST /v2/console/games` — a new game. `game_key`, `api_key` and `client_key` are
/// minted by the server: a tenant credential that a client could choose is a credential
/// that a client could guess.
///
/// The config arrives in either of two spellings: the nested document
/// (`config: { "tick_rate": … }`) or the flat fields of the product plan
/// (`tick_rate: …` alongside `name`). A flat field overrides the same key inside
/// `config` when both are named; unknown keys in `config` are kept as written.
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

/// `PUT /v2/console/games/{gameId}` — a patch: an absent field keeps its stored
/// value. `game_key` and `api_key` are absent by construction; the identity of a
/// tenant is not editable, and rotating its secret is its own (future) action.
///
/// The four config fields ride flat exactly as registration accepts them (the
/// "flat fields win server-side" contract the dashboard's register sheet
/// documents); they merge into the stored config the same way `create_config`
/// does. Without them, a patch that only names `config` would be the sole way
/// to move a tick rate — and the dashboard's configure sheet does not speak it.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateGameRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub engine_type: Option<String>,
    /// `active`, `suspended` or `archived`.
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

/// The columns of a game row, secret excluded. This is the shape every service query
/// reads, and what the module plane validates a tenant against. `client_key` is a
/// publishable credential and travels with it; `api_key` never does.
#[derive(Debug, Clone, sqlx::FromRow)]
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

/// A game as the console displays it.
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

/// `GET /v2/console/games/{gameId}` — one game, its secret revealed, plus what modules
/// it owns. Opening the detail view is the only place a console operator reads the
/// API key back, which is why the module counts come with it rather than as a second
/// round trip.
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

/// `GET /v2/console/games` — the whole listing with its size, so a table can render
/// "3 of 12" without a second request.
#[derive(Debug, Serialize)]
pub struct GameListDto {
    pub games: Vec<GameDto>,
    pub total: i64,
}

/// `DELETE /v2/console/games/{gameId}` — the archive confirmation. The row is kept, so
/// the reply says what happened to it rather than pretending it vanished.
#[derive(Debug, Serialize)]
pub struct ArchiveGameResponse {
    pub archived: bool,
}

/// `GET /v2/console/games/{gameId}/unity-settings` — the connection companion of the
/// generated `CYGServerSettings.asset`, in the plan's camelCase wire shape.
/// `apiKey` is present only when the caller asked with `?reveal=true`; the secret is
/// never handed out on a settings download by default.
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
