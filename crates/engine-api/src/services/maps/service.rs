use super::*;

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
pub struct MapService;
#[singleton]
impl MapService {
    fn new() -> Self {
        Self
    }
}
pub(super) const MAX_MAP_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_JSON_OBSTACLES: usize = 50_000;
pub(super) const OXCL_MAGIC: &[u8; 4] = b"OXCL";
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub struct MapUploadResponse {
    pub map: MapDto,
}
