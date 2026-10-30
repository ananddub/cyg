use axum::Json;
use chrono::Utc;
use serde_json::{Value, json};

use super::load_board;
use super::rank::rank_of;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::leaderboard::{RecordRequest, RecordResponse};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::GetLeaderboardRecordParams;
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::services::deps::q;
use crate::utils::leaderboard::statement::Write;
use crate::utils::leaderboard::types::{
    AUTH_WRITE, BAD_RECORD, DB_DECREMENT, DB_INCREMENT, DB_SET, INVALID_ID, WRITE_FAILED,
    ascending, parse_metadata, ranks_enabled, resolve_operator,
};
use crate::utils::leaderboard::window::record_window;

mod tuple_params;
mod upsert;

use tuple_params::*;
use upsert::*;
