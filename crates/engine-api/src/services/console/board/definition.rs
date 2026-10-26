//! The write shape of a board row: the hand-run statements, the request merged
//! over a fallback, and the checks a database constraint would otherwise answer.

use chrono::{DateTime, Utc};

use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::cron::Schedule;

/// A tournament runs in rounds, and a round needs a length.
pub const BAD_DURATION: &str = "A tournament needs a duration of at least one second.";
pub const ID_MISSING: &str = "Tournament ID must be provided.";
pub const ID_TAKEN: &str = "Tournament id is already taken.";
pub const BAD_LIMITS: &str = "Both max_size and max_num_score must be positive.";
pub const BAD_SCHEDULE: &str = "leaderboard reset schedule is not a cron expression";

/// A board row created through the console is a tournament: `category = 1` is
/// written, never taken from the body.
pub const INSERT_SQL: &str = "
INSERT INTO leaderboard
    (id, title, description, operator, sort_order, reset_schedule, metadata,
     authoritative, category, duration, start_time, end_time, join_required,
     max_size, max_num_score, game_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, $9, $10, $11, $12, $13, $14, $15)
RETURNING *;";

/// The tournament's own fields; `id` and `category` stay where they are, so an
/// update cannot rename a board or turn a leaderboard into a tournament.
pub const UPDATE_SQL: &str = "
UPDATE leaderboard
SET title = $2, description = $3, operator = $4, sort_order = $5,
    reset_schedule = $6, metadata = $7, authoritative = $8, duration = $9,
    start_time = $10, end_time = $11, join_required = $12, max_size = $13,
    max_num_score = $14
WHERE id = $1 AND category = 1
RETURNING *;";

/// The one statement that ends a board and, by the schema's cascade, every
/// record on it.
pub const DELETE_SQL: &str = "DELETE FROM leaderboard WHERE id = $1;";

/// Everything one of the write statements binds, defaults resolved.
pub struct BoardWrite {
    pub id: String,
    pub title: String,
    pub description: String,
    pub operator: i16,
    pub sort_order: i16,
    pub reset_schedule: Option<String>,
    pub metadata: serde_json::Value,
    pub authoritative: bool,
    pub duration: i32,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub join_required: bool,
    pub max_size: i32,
    pub max_num_score: i32,
}

impl BoardWrite {
    /// The request over the fallback — a board row on an update, the column
    /// defaults on a create. The id is decided by the caller: the body's on a
    /// create, the path's on an update.
    pub fn resolve(
        body: &TournamentWriteRequest,
        mut base: BoardWrite,
        id: String,
    ) -> Result<BoardWrite, V2Error> {
        base.id = id;
        if let Some(title) = &body.title {
            base.title = title.clone();
        }
        if let Some(description) = &body.description {
            base.description = description.clone();
        }
        base.operator = body.operator.unwrap_or(base.operator);
        base.sort_order = body.sort_order.unwrap_or(base.sort_order);
        base.authoritative = body.authoritative.unwrap_or(base.authoritative);
        base.join_required = body.join_required.unwrap_or(base.join_required);
        base.metadata = body.metadata.clone().unwrap_or(base.metadata);
        base.duration = body.duration.unwrap_or(base.duration);
        base.max_size = body.max_size.unwrap_or(base.max_size);
        base.max_num_score = body.max_num_score.unwrap_or(base.max_num_score);
        base.start_time = body.start_time.unwrap_or(base.start_time);
        base.end_time = body.end_time.unwrap_or(base.end_time);
        if base.duration < 1 {
            return Err(V2Error::invalid_argument(BAD_DURATION));
        }
        if base.max_size < 1 || base.max_num_score < 1 {
            return Err(V2Error::invalid_argument(BAD_LIMITS));
        }
        if let Some(expression) = body.reset_schedule.as_deref().filter(|s| !s.is_empty()) {
            if Schedule::parse(expression).is_none() {
                return Err(V2Error::invalid_argument(BAD_SCHEDULE));
            }
            base.reset_schedule = Some(expression.to_owned());
        } else if body.reset_schedule.is_some() {
            // An explicit empty schedule clears the board's round clock.
            base.reset_schedule = None;
        }
        Ok(base)
    }

    /// The board row's own values as a write: an update that leaves a field out
    /// keeps what is stored rather than resetting it.
    pub fn stored(board: &Leaderboard) -> BoardWrite {
        BoardWrite {
            id: board.id.clone(),
            title: board.title.clone(),
            description: board.description.clone(),
            operator: board.operator,
            sort_order: board.sort_order,
            reset_schedule: board.reset_schedule.clone(),
            metadata: board.metadata.clone(),
            authoritative: board.authoritative,
            duration: board.duration,
            start_time: board.start_time,
            end_time: board.end_time,
            join_required: board.join_required,
            max_size: board.max_size,
            max_num_score: board.max_num_score,
        }
    }

    /// The schema's column defaults, which is what a create without a field means.
    pub fn defaults() -> BoardWrite {
        BoardWrite {
            id: String::new(),
            title: String::new(),
            description: String::new(),
            operator: 0,
            sort_order: 1,
            reset_schedule: None,
            metadata: serde_json::json!({}),
            authoritative: false,
            duration: 0,
            start_time: Utc::now(),
            end_time: crate::services::console::never(),
            join_required: false,
            max_size: 100_000_000,
            max_num_score: 1_000_000,
        }
    }
}
