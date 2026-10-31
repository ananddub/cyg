use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dto::console::leaderboard::TournamentWriteRequest;
use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::cron::Schedule;

pub const BAD_DURATION: &str = "A tournament needs a duration of at least one second.";
pub const ID_MISSING: &str = "Tournament ID must be provided.";
pub const ID_TAKEN: &str = "Tournament id is already taken.";
pub const BAD_LIMITS: &str = "Both max_size and max_num_score must be positive.";
pub const BAD_SCHEDULE: &str = "leaderboard reset schedule is not a cron expression";

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

pub fn tournament_row(write: &BoardWrite, game: Option<Uuid>, now: DateTime<Utc>) -> Leaderboard {
    Leaderboard {
        id: write.id.clone(),
        authoritative: write.authoritative,
        sort_order: write.sort_order,
        operator: write.operator,
        reset_schedule: write.reset_schedule.clone(),
        metadata: write.metadata.clone(),
        create_time: now,
        category: 1,
        description: write.description.clone(),
        duration: write.duration,
        end_time: write.end_time,
        join_required: write.join_required,
        max_size: write.max_size,
        max_num_score: write.max_num_score,
        title: write.title.clone(),
        size: 0,
        start_time: write.start_time,
        enable_ranks: Some(true),
        game_id: game,
    }
}

impl BoardWrite {
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
            base.reset_schedule = None;
        }
        Ok(base)
    }

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
