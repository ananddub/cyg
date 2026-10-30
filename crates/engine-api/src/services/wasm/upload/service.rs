use super::*;

pub const REQUIRED_EXPORTS: [&str; 8] = [
    "on_alloc",
    "on_create",
    "on_input",
    "on_join",
    "on_leave",
    "on_load",
    "on_save",
    "on_tick",
];
pub(super) fn module_key(game_id: Uuid, version: i32) -> String {
    format!("wasm/{game_id}/{version}.wasm")
}
pub struct InsertModuleParams {
    pub filename: String,
    pub binary_bytes: Vec<u8>,
    pub source_code: Option<String>,
    pub sha256: String,
}
pub(super) const WASM_BACKEND_MODE: &str = "wasm";
pub struct WasmService;
#[singleton]
impl WasmService {
    fn new() -> Self {
        Self
    }
}
pub(super) fn module_dto(row: ListRoomModulesRow) -> ModuleDto {
    ModuleDto {
        id: row.id,
        version: row.version,
        filename: row.filename,
        sha256: row.sha256,
        is_active: row.is_active,
        size_bytes: row.size_bytes,
        created_at: row.created_at,
        has_source: row.has_source != 0,
    }
}
