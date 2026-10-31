use auto_di::singleton;
use chrono::{DateTime, Utc};
use engine_bus::MapReload;
use engine_config::{BootMaps, MapAsset, MapFormat};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ApiState;
use crate::db::time;
use crate::error::V2Error;
use crate::repositories::game_maps::{ListGameMapsRow, StoredGameMapRow};

mod q;
mod list_maps;
mod delete_map;
mod tests;

pub use q::*;
pub(crate) use crate::services::deps::{blobs, q};
