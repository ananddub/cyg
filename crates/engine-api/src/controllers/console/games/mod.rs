use auto_route::controller;
use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, BoolFlag};
use crate::current_state;
use crate::dto::console::games::{
    ArchiveGameResponse, CreateGameRequest, UnityConnectionDto, UpdateGameRequest,
};
use crate::error::V2Error;
use crate::services::games::GamesService;

mod query_params;
mod unity_asset_yaml;

pub use query_params::*;
use unity_asset_yaml::*;
