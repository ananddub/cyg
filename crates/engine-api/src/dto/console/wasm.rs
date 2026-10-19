//! Dynamic WASM room modules (`/v2/console/games/{gameId}/wasm*`): the stored
//! module rows the console lists, the ABI report an upload answers with, and
//! the small request bodies the activation endpoints take.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The module's ABI as the upload endpoint enforces it: which of the guest
/// exports the host actually calls were found, and which were not. A module is
/// `valid` only when every required export is present and the bytes parse.
#[derive(Debug, Serialize)]
pub struct AbiReport {
    pub valid: bool,
    /// `\0asm` + version header — the cheapest "this is a wasm module" test,
    /// reported separately so a truncated upload reads differently from a
    /// wrong-ABI one.
    pub wasm_magic_ok: bool,
    /// Required exports the module provides.
    pub exports_found: Vec<String>,
    /// Required exports the module lacks; the upload refusal names these.
    pub exports_missing: Vec<String>,
    /// Every export the module declares, required or not.
    pub export_count: u32,
    pub size_bytes: usize,
}

/// One stored `room_modules` row, without its binary — a list endpoint has no
/// reason to ship megabytes of bytecode over the console.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleDto {
    pub id: Uuid,
    pub version: i32,
    pub filename: String,
    pub sha256: String,
    pub is_active: bool,
    /// `octet_length(binary_bytes)`: the row's payload size, computed in the
    /// database so the bytes themselves never leave it for a listing.
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub has_source: bool,
}

/// `POST …/upload` — the stored row plus the ABI verdict it was validated
/// against. Both, because the operator wants to see the version *and* proof
/// the contract was checked.
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub module: ModuleDto,
    pub abi: AbiReport,
}

/// `POST …/activate` and `POST …/rollback` — the stored version to make
/// active. Rollback is activation of an older number; the endpoints differ
/// only in the reason they publish to the bus.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    #[serde(default)]
    pub version: i32,
}
