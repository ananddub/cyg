mod definition;

use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::db::{DbError, Key};
use crate::dto::Empty;
use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::dto::tournament::TournamentResponse;
use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::types::{NOT_FOUND as BOARD_NOT_FOUND, is_tournament};
use crate::utils::tournament::types::NOT_FOUND;

use definition::{BoardWrite, ID_MISSING, ID_TAKEN};

mod console_service;
mod remove_board;

use remove_board::*;
