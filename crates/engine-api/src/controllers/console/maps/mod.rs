use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::error::V2Error;
use crate::services::maps::MapService;
use engine_config::MapFormat;

mod controller;
mod routes;

pub use controller::*;
