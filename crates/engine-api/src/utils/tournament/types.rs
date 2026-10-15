//! Shared tournament pieces: gateway messages, limits, operator numbering and the page cursor codec.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{INVALID_ARGUMENT, UNKNOWN, V2Error};
use crate::utils::leaderboard::{
    DB_BEST, DB_DECREMENT, DB_INCREMENT, DB_SET, number, owner_ids,
};

pub const ID_MISSING: &str = "Tournament ID must be provided";
pub const ID_MISSING_DOT: &str = "Tournament ID must be provided.";
pub const BAD_ID: &str = "Invalid tournament ID.";
pub const NOT_FOUND: &str = "Tournament not found.";
pub const ENDED: &str = "Tournament has ended.";
pub const WRITE_ENDED: &str = "Tournament not found or has ended.";
pub const BAD_LIMIT: &str = "Invalid limit - limit must be between 1 and 100.";
pub const BAD_PAGE_LIMIT: &str = "Limit must be between 1 and 100.";
pub const BAD_OWNERS: &str = "One or more owner IDs are invalid.";
pub const BAD_OWNER: &str = "Invalid owner ID provided.";
pub const NO_OWNER: &str = "Owner ID must be provided for a haystack query.";
pub const BAD_EXPIRY: &str = "Invalid expiry - expiry must be a UNIX timestamp.";
pub const BAD_RECORD: &str = "Invalid input, record score value is required.";
pub const BAD_CATEGORY_END: &str = "Tournament category end must be >=0 and <128.";
pub const BAD_CATEGORY_ORDER: &str = "Tournament category end must be greater than category start.";
pub const BAD_TIME_ORDER: &str = "Tournament end time must be greater than start time.";
pub const CURSOR_INVALID: &str = "leaderboard cursor invalid";
pub const LIST_FAILED: &str = "Error listing records from tournament.";
pub const HAYSTACK_FAILED: &str = "Error querying records from leaderboard.";
pub const PAGE_FAILED: &str = "Error listing tournaments.";
pub const WRITE_FAILED: &str = "Error writing score to tournament.";
pub const JOIN_FAILED: &str = "Error while trying to join tournament.";
pub const DELETE_FAILED: &str = "Error deleting score from tournament.";
pub const AUTH_WRITE: &str = "Tournament only allows authoritative score submissions.";
pub const INACTIVE_JOIN: &str = "Tournament is not active and cannot accept new joins.";
pub const INACTIVE_WRITE: &str = "Tournament is not active and cannot accept new scores.";
pub const MAX_SIZE: &str = "Tournament has reached max size.";
pub const MAX_SIZE_JOIN: &str = "Tournament cannot be joined as it has reached its max size.";
pub const MAX_ATTEMPTS: &str = "Reached allowed max number of score attempts.";
pub const JOIN_REQUIRED: &str = "Must join tournament before attempting to write value.";

pub const MAX_LIMIT: i64 = 100;
pub const DEFAULT_RECORD_LIMIT: i64 = 10;
pub const DEFAULT_PAGE_LIMIT: i64 = 100;
pub const DEFAULT_CATEGORY_END: i64 = 127;
pub const DEFAULT_TIME: i64 = -1;

/// A query parameter the gateway itself parses, and cannot.
pub const BAD_NUMBER: &str = "invalid Argument";

/// The limit a route asks for, bounded the gateway way: absent takes the default, outside 1-100 is the caller's.
pub fn limit(raw: Option<&str>, default: i64, message: &str) -> Result<i64, V2Error> {
    let value = defaulted(raw, default, message)?;
    if (1..=MAX_LIMIT).contains(&value) {
        Ok(value)
    } else {
        Err(V2Error::invalid_argument(message))
    }
}

pub fn defaulted(raw: Option<&str>, default: i64, message: &str) -> Result<i64, V2Error> {
    match raw {
        Some(text) => number(Some(text), message),
        None => Ok(default),
    }
}

/// `leaderboard.operator` numbering read back as the `api.Operator` the wire
/// carries: the enum starts one higher and reserves 0 for "no override".
pub fn api_operator(stored: i16) -> u32 {
    match stored {
        DB_BEST => 1,
        DB_SET => 2,
        DB_INCREMENT => 3,
        DB_DECREMENT => 4,
        _ => 0,
    }
}

/// the `api.Operator` override back into the column's numbering; anything
/// the enum does not name is the write's generic failure.
pub fn db_operator(override_operator: i64) -> Result<Option<i16>, V2Error> {
    match override_operator {
        0 => Ok(None),
        1 => Ok(Some(DB_BEST)),
        2 => Ok(Some(DB_SET)),
        3 => Ok(Some(DB_INCREMENT)),
        4 => Ok(Some(DB_DECREMENT)),
        _ => Err(V2Error::internal(WRITE_FAILED)),
    }
}

/// The last tournament id a page returned, JSON base64url like every other cursor here.
#[derive(Debug, Serialize, Deserialize)]
struct CursorBody {
    id: String,
}

pub fn page_cursor(id: &str) -> String {
    let json = serde_json::to_vec(&CursorBody { id: id.to_string() })
        .unwrap_or_else(|_| Vec::new());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

/// An undecodable cursor is the raw sentinel error BaaS hands back unwrapped,
/// which the gateway prints as an unknown code rather than an argument error.
pub fn decode_page_cursor(raw: &str) -> Result<Option<String>, V2Error> {
    if raw.is_empty() {
        return Ok(None);
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .or_else(|_| {
            base64::engine::general_purpose::URL_SAFE.decode(raw)
        })
        .map_err(|_| V2Error::new(UNKNOWN, CURSOR_INVALID))?;
    let body: CursorBody =
        serde_json::from_slice(&bytes).map_err(|_| V2Error::new(UNKNOWN, CURSOR_INVALID))?;
    Ok(Some(body.id))
}

/// The owners a listing names, in the same two spellings the gateway allows.
pub fn owners(raw: Option<&Value>, message: &str) -> Result<Vec<Uuid>, V2Error> {
    owner_ids(raw, message)
}

/// The shared leaderboard listing reports a failure in its own words; a tournament
/// route restates it, keeping only the cursor's message, which both routes spell identically.
pub fn list_failure(error: V2Error, message: &str) -> V2Error {
    if error.kind == INVALID_ARGUMENT {
        return error;
    }
    V2Error::internal(message)
}
