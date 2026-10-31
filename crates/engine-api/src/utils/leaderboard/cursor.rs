use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::types::BAD_CURSOR;
use crate::error::V2Error;
use crate::repositories::models::LeaderboardRecord;
use crate::services::social::{cursor_decode, cursor_encode, uid};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    pub is_next: bool,
    pub leaderboard_id: String,
    pub expiry_time: i64,
    pub score: i64,
    pub subscore: i64,
    pub owner_id: String,
    pub rank: i64,
}

impl Cursor {
    pub fn at_record(
        board_id: &str,
        window: i64,
        record: &LeaderboardRecord,
        rank: i64,
        is_next: bool,
    ) -> Cursor {
        Cursor {
            is_next,
            leaderboard_id: board_id.to_string(),
            expiry_time: window,
            score: record.score,
            subscore: record.subscore,
            owner_id: uid(&record.owner_id),
            rank,
        }
    }

    pub fn encode(&self) -> Result<String, V2Error> {
        let value = serde_json::to_value(self)
            .map_err(|_| V2Error::internal("Error creating leaderboard records list cursor"))?;
        Ok(cursor_encode(&value))
    }

    pub fn decode(raw: &str, board_id: &str, window: i64) -> Result<Option<Cursor>, V2Error> {
        if raw.is_empty() {
            return Ok(None);
        }
        let value = cursor_decode(raw, BAD_CURSOR)?;
        let cursor: Cursor =
            serde_json::from_value(value).map_err(|_| V2Error::invalid_argument(BAD_CURSOR))?;
        if cursor.leaderboard_id != board_id || cursor.expiry_time != window {
            return Err(V2Error::invalid_argument(BAD_CURSOR));
        }
        Ok(Some(cursor))
    }

    pub fn owner(&self) -> Result<Uuid, V2Error> {
        Uuid::parse_str(&self.owner_id).map_err(|_| V2Error::invalid_argument(BAD_CURSOR))
    }
}
