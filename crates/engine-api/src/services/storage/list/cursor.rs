//! A cursor is the base64url of a JSON object holding the last returned row's
//! `(key, read, user_id)`; a field the current listing does not compare on is
//! written anyway and ignored, as it is in BaaS.

use serde_json::Value;
use uuid::Uuid;

use crate::utils::storage::CURSOR_MESSAGE;
use crate::error::V2Error;
use crate::services::social;

pub(super) struct Resumed {
    pub(super) key: String,
    pub(super) read: i16,
    pub(super) user_id: Uuid,
}

/// Decode a paging cursor, rejecting an empty key but tolerating the fields the
/// listing does not compare on.
pub(super) fn resume(raw: Option<&String>) -> Result<Option<Resumed>, V2Error> {
    let Some(raw) = raw.filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let value = social::cursor_decode(raw, CURSOR_MESSAGE)?;
    let key = value
        .get("key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| V2Error::invalid_argument(CURSOR_MESSAGE))?;
    Ok(Some(Resumed {
        key: key.to_owned(),
        read: value
            .get("read")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 2) as i16,
        user_id: social::cursor_uuid(&value, "user_id"),
    }))
}
