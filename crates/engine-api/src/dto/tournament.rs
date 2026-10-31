use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::{Deadlines, calculate, prev_reset, stamp};
use crate::utils::tournament::types::api_operator;

fn zero(value: &u32) -> bool {
    *value == 0
}

fn absent(value: &Option<u32>) -> bool {
    value.is_none()
}

fn no_string(value: &Option<String>) -> bool {
    value.is_none()
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize)]
pub struct TournamentResponse {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "absent")]
    pub category: Option<u32>,
    #[serde(skip_serializing_if = "absent")]
    pub sort_order: Option<u32>,
    #[serde(skip_serializing_if = "absent")]
    pub size: Option<u32>,
    #[serde(skip_serializing_if = "absent")]
    pub max_size: Option<u32>,
    #[serde(skip_serializing_if = "absent")]
    pub max_num_score: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    pub can_enter: bool,
    #[serde(skip_serializing_if = "zero")]
    pub end_active: u32,
    #[serde(skip_serializing_if = "zero")]
    pub next_reset: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    pub create_time: String,
    pub start_time: String,
    #[serde(skip_serializing_if = "no_string")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "zero")]
    pub duration: u32,
    #[serde(skip_serializing_if = "zero")]
    pub start_active: u32,
    #[serde(skip_serializing_if = "zero")]
    pub prev_reset: u32,
    #[serde(skip_serializing_if = "zero")]
    pub operator: u32,
    #[serde(skip_serializing_if = "is_false")]
    pub authoritative: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub join_required: bool,
}

impl TournamentResponse {
    pub fn from_board(
        board: &Leaderboard,
        now: DateTime<Utc>,
    ) -> Result<TournamentResponse, V2Error> {
        let Deadlines {
            start_active,
            end_active,
            expiry,
        } = calculate(board, now)?;
        let now_unix = now.timestamp();
        let can_enter =
            start_active <= now_unix && end_active >= now_unix && board.size < board.max_size;

        Ok(TournamentResponse {
            id: board.id.clone(),
            title: board.title.clone(),
            description: board.description.clone(),
            category: count(i64::from(board.category)),
            sort_order: count(i64::from(board.sort_order)),
            size: count(i64::from(board.size)),
            max_size: count(i64::from(board.max_size)),
            max_num_score: count(i64::from(board.max_num_score)),
            can_enter,
            end_active: uint32(end_active),
            next_reset: uint32(expiry),
            metadata: board.metadata.to_string(),
            create_time: stamp(board.create_time),
            start_time: stamp(board.start_time),
            end_time: (board.end_time.timestamp() != 0).then(|| stamp(board.end_time)),
            duration: uint32(i64::from(board.duration)),
            start_active: uint32(start_active),
            prev_reset: uint32(prev_reset(board, now)?),
            operator: api_operator(board.operator),
            authoritative: board.authoritative,
            join_required: board.join_required,
        })
    }
}

fn count(value: i64) -> Option<u32> {
    (value != 0).then(|| uint32(value))
}

fn uint32(value: i64) -> u32 {
    value as u32
}

#[derive(Debug, Default, Serialize)]
pub struct TournamentListResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tournaments: Vec<TournamentResponse>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cursor: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub category_start: Option<String>,
    #[serde(default)]
    pub category_end: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RecordsQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub owner_ids: Option<Value>,
    #[serde(default)]
    pub expiry: Option<String>,
}
