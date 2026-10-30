use auto_di::singleton;
use chrono::Utc;
use engine_bus::{ReloadKind, WasmReload};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ApiState;
use crate::db::time;
use crate::dto::console::wasm::{AbiReport, ModuleDto, UploadResponse};
use crate::error::V2Error;
use crate::repositories::games::ListRoomModulesRow;

mod activate_module;
mod boot_module;
mod require_wasm_mode;
mod service;
mod sha256_hex;
mod validate_binary;

pub use service::*;
use sha256_hex::*;
