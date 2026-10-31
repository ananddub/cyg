use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListGameMapsRow {
    pub id: Uuid,
    pub version: i32,
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub is_active: bool,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadActiveGameMapsRow {
    pub name: String,
    pub format: String,
    pub version: i32,
    pub blob_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeletedGameMapRow {
    pub blob_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredGameMapRow {
    pub id: Uuid,
    pub version: i32,
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub blob_key: Option<String>,
}

pub(crate) const GAME_MAP_COLUMNS: &str = "id, version, name, format, sha256, is_active, \
     IF size_bytes IS NONE THEN 0 ELSE size_bytes END AS size_bytes, created_at";
