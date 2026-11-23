use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct AbiReport {
    pub valid: bool,
    pub wasm_magic_ok: bool,
    pub exports_found: Vec<String>,
    pub exports_missing: Vec<String>,
    pub export_count: u32,
    pub size_bytes: usize,
}

#[derive(poem_openapi::Object, Debug, Clone, Serialize)]
pub struct ModuleDto {
    pub id: Uuid,
    pub version: i32,
    pub filename: String,
    pub sha256: String,
    pub is_active: bool,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub has_source: bool,
}

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct UploadResponse {
    pub module: ModuleDto,
    pub abi: AbiReport,
}

#[derive(poem_openapi::Object, Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    #[serde(default)]
    pub version: i32,
}
