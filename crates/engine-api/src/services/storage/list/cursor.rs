use serde_json::Value;

use crate::error::V2Error;
use crate::repositories::storage::StorageCursor;
use crate::services::social;
use crate::utils::storage::CURSOR_MESSAGE;

pub(super) fn resume(raw: Option<&String>) -> Result<Option<StorageCursor>, V2Error> {
    let Some(raw) = raw.filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let value = social::cursor_decode(raw, CURSOR_MESSAGE)?;
    let key = value
        .get("key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| V2Error::invalid_argument(CURSOR_MESSAGE))?;
    Ok(Some(StorageCursor {
        key: key.to_owned(),
        read: value
            .get("read")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 2) as i16,
        user_id: social::cursor_uuid(&value, "user_id"),
    }))
}
