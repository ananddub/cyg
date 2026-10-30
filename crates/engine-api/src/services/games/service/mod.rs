use auto_di::singleton;
use axum::http::HeaderMap;
use rand::Rng;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::games::{
    CreateGameRequest, ENGINE_TYPES, GAME_STATUSES, GameConfig, GameDetailDto, GameDto,
    GameListDto, GameRow, UnityConnectionDto, UpdateGameRequest,
};
use crate::error::V2Error;
use crate::utils::status::client_host;

use super::schema::DEFAULT_GAME_CONFIG_JSON;
use crate::repositories::games::{InsertGameParams, ListGamesRow, UpdateGameParams};

mod create_game;
mod update_game;
mod game_name;
mod service;

pub use service::*;
pub use game_name::*;
