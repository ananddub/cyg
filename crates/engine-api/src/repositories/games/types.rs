use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InsertGameRow {
    pub id: Uuid,
    pub game_key: String,
    pub name: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub status: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub type ListGamesRow = InsertGameRow;

pub type FindGameByIdRow = InsertGameRow;

pub type FindGameByKeyRow = InsertGameRow;

pub type UpdateGameRow = InsertGameRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListRoomModulesRow {
    pub id: Uuid,
    pub version: i32,
    pub filename: String,
    pub sha256: String,
    pub is_active: bool,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub has_source: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindRoomModuleBytesRow {
    pub filename: String,
    pub blob_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActiveRoomModuleRow {
    pub version: i32,
    pub sha256: String,
    pub blob_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeletedRoomModuleRow {
    pub blob_key: Option<String>,
}

pub struct InsertGameParams {
    pub game_key: String,
    pub name: String,
    pub api_key: String,
    pub client_key: String,
    pub engine_type: String,
    pub backend_mode: String,
    pub config: serde_json::Value,
}

pub struct UpdateGameParams {
    pub name: String,
    pub engine_type: String,
    pub status: String,
    pub config: serde_json::Value,
    pub backend_mode: String,
    pub id: Uuid,
}

pub(crate) use crate::repositories::db::{WriteResult, affected};

pub(crate) const GAME_COLUMNS: &str = "id, game_key, name, client_key, engine_type, backend_mode, status, \
     config, created_at, updated_at";

pub(crate) const ROOM_MODULE_COLUMNS: &str = "id, version, filename, sha256, is_active, \
     IF size_bytes IS NONE THEN 0 ELSE size_bytes END AS size_bytes, created_at, \
     IF source_code IS NONE OR source_code IS NULL THEN 0 ELSE 1 END AS has_source";

impl From<Games> for InsertGameRow {
    fn from(row: Games) -> Self {
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

pub(super) fn new_game(id: Uuid, p: &InsertGameParams) -> Games {
    let now = Utc::now();
    Games {
        id,
        game_key: p.game_key.clone(),
        name: p.name.clone(),
        api_key: p.api_key.clone(),
        client_key: p.client_key.clone(),
        engine_type: p.engine_type.clone(),
        backend_mode: p.backend_mode.clone(),
        status: "active".to_string(),
        config: p.config.clone(),
        created_at: now,
        updated_at: now,
    }
}
