use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::{GameScope, Page};
use crate::dto::console::account::{
    AccountExport, AddNoteRequest, BanAccountRequest, UpdateWalletRequest,
};
use crate::services::console::ConsoleService;
use crate::utils::guards;

mod controller;
mod routes;

use controller::*;
pub use routes::*;
