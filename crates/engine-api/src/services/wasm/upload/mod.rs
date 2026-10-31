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

mod q;
mod require_wasm_mode;
mod activate_module;
mod validate_binary;
mod sha256_hex;

pub use q::*;
pub(crate) use crate::services::deps::{blobs, q};
use sha256_hex::*;
