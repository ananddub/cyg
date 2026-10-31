use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request};
use axum::response::{IntoResponse, Response};
use engine_bus::ReloadKind;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::wasm::ActivateRequest;
use crate::error::V2Error;
use crate::services::wasm::WasmService;

mod controller;
mod routes;

pub use controller::*;
