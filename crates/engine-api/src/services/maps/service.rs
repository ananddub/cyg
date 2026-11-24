use super::*;
use crate::services::plane::Plane;
use std::sync::Arc;

pub(super) fn map_key(game_id: Uuid, version: i32, format: &str) -> String {
    format!("maps/{game_id}/{version}.{format}")
}
pub(super) fn map_dto(row: ListGameMapsRow) -> MapDto {
    MapDto {
        id: row.id,
        version: row.version,
        name: row.name,
        format: row.format,
        sha256: row.sha256,
        is_active: row.is_active,
        size_bytes: row.size_bytes,
        created_at: row.created_at,
    }
}
pub struct MapService {
    pub(crate) plane: Arc<Plane>,
}
#[singleton]
impl MapService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
pub(super) const MAX_MAP_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_JSON_OBSTACLES: usize = 50_000;
pub(super) const OXCL_MAGIC: &[u8; 4] = b"OXCL";
#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct MapDto {
    pub id: Uuid,
    pub version: i32,
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub is_active: bool,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}
#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct MapUploadResponse {
    pub map: MapDto,
}

/// The multipart fields the console's map upload accepts.
#[derive(poem_openapi::Object)]
pub struct MapUploadRequest {
    pub file: crate::dto::common::BinaryField,
    /// One of `oxcl`, `map2d`, `map3d`, `navmesh`; omitted means infer from the
    /// filename extension.
    pub format: Option<String>,
    /// Display name; omitted means the uploaded filename.
    pub name: Option<String>,
}
