//! Shared leaderboard pieces: gateway messages, operator numbering and the pure board facts.

use serde_json::Value;
use uuid::Uuid;

use crate::error::V2Error;
use crate::repositories::models::Leaderboard;

pub const INVALID_ID: &str = "Invalid leaderboard ID.";
pub const NOT_FOUND: &str = "Leaderboard not found.";
pub const AUTH_WRITE: &str = "Leaderboard only allows authoritative score submissions.";
pub const AUTH_DELETE: &str = "Leaderboard only allows authoritative score deletions.";
pub const BAD_RECORD: &str = "Invalid input, record score value is required.";
pub const BAD_METADATA: &str = "Metadata value must be JSON, if provided.";
pub const LIST_LIMIT: &str = "Invalid limit - limit must be between 1 and 1000.";
pub const OWNER_LIMIT: &str = "Invalid limit - limit must be between 1 and 100.";
pub const BAD_OWNERS: &str = "One or more owner IDs are invalid.";
pub const BAD_OWNER: &str = "Invalid owner ID provided.";
pub const NO_OWNER: &str = "Owner ID must be provided for a haystack query.";
pub const BAD_CURSOR: &str = "Cursor is invalid or expired.";
pub const BAD_EXPIRY: &str = "Invalid expiry - expiry must be a UNIX timestamp.";
pub const WRITE_FAILED: &str = "Error writing score to leaderboard.";
pub const LIST_FAILED: &str = "Error listing records from leaderboard.";
pub const HAYSTACK_FAILED: &str = "Error querying records from leaderboard.";
pub const DELETE_FAILED: &str = "Error deleting score from leaderboard.";

/// `api.Operator` as a client sends it; `NO_OVERRIDE` defers to the board.
pub const OP_BEST: i64 = 1;
pub const OP_SET: i64 = 2;
pub const OP_INCREMENT: i64 = 3;
pub const OP_DECREMENT: i64 = 4;

/// The `leaderboard.operator` column's own numbering.
pub const DB_BEST: i16 = 0;
pub const DB_SET: i16 = 1;
pub const DB_INCREMENT: i16 = 2;
pub const DB_DECREMENT: i16 = 3;
pub const SORT_ASCENDING: i16 = 0;

pub const MAX_LIST_LIMIT: i64 = 1000;
pub const MAX_OWNER_LIMIT: i64 = 100;

/// Score ranking direction: the column default is descending.
pub fn ascending(board: &Leaderboard) -> bool {
    board.sort_order == SORT_ASCENDING
}

pub fn ranks_enabled(board: &Leaderboard) -> bool {
    board.enable_ranks != Some(false)
}

pub fn is_tournament(board: &Leaderboard) -> bool {
    board.duration > 0
}

/// the override table and its unmapped failure: an operator outside the enum
/// is the write's generic internal error, not an `InvalidArgument`.
pub fn resolve_operator(board: &Leaderboard, override_operator: i64) -> Result<i16, V2Error> {
    if override_operator == 0 {
        return Ok(board.operator);
    }
    match override_operator {
        OP_BEST => Ok(DB_BEST),
        OP_SET => Ok(DB_SET),
        OP_INCREMENT => Ok(DB_INCREMENT),
        OP_DECREMENT => Ok(DB_DECREMENT),
        _ => Err(V2Error::internal(WRITE_FAILED)),
    }
}

/// The gateway reads a nullable int64 out of query text and a JSON body
/// accepts both spellings, so an unparseable value is the caller's error.
pub fn number(raw: Option<&str>, message: &str) -> Result<i64, V2Error> {
    raw.unwrap_or("0")
        .trim()
        .parse::<i64>()
        .map_err(|_| V2Error::invalid_argument(message))
}

/// The gateway's metadata test: valid JSON whose first token is an object brace.
pub fn parse_metadata(raw: &str) -> Result<Value, V2Error> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') {
        return Err(V2Error::invalid_argument(BAD_METADATA));
    }
    serde_json::from_str(trimmed).map_err(|_| V2Error::invalid_argument(BAD_METADATA))
}

/// A repeated string field arrives comma-separated, as a list, or both.
pub fn owner_ids(raw: Option<&Value>, message: &str) -> Result<Vec<Uuid>, V2Error> {
    let tokens = crate::services::social::tokens(raw);
    tokens
        .iter()
        .map(|t| Uuid::parse_str(t).map_err(|_| V2Error::invalid_argument(message)))
        .collect()
}
