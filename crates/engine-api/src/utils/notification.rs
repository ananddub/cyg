//! Notification helpers: the paging cursor codec and the id-list flattening.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::models::Notification;
use crate::error::V2Error;
use crate::services::social::{cursor_decode, cursor_encode, uid};

pub(crate) const LIMIT_RANGE: &str = "Invalid limit - limit must be between 1 and 100.";
/// The gateway wraps every failure of the list query — a bad cursor included —
/// in this one internal error, so a client cannot tell them apart.
pub(crate) const LIST_FAILED: &str = "Error retrieving notifications.";

pub(crate) fn encode_cursor_at(create_time: DateTime<Utc>, id: Uuid) -> String {
    cursor_encode(&json!({
        "create_time": create_time.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "id": uid(&id),
    }))
}

pub(crate) fn encode_cursor(n: &Notification) -> String {
    encode_cursor_at(n.create_time, n.id)
}

/// Where to resume: `None` is the start of the list. A cursor naming no row is
/// the empty cursor BaaS hands back with an empty page, so it reads the same.
pub(crate) fn decode_cursor(raw: Option<&str>) -> Result<Option<(DateTime<Utc>, Uuid)>, V2Error> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let v = cursor_decode(raw, LIST_FAILED).map_err(|_| V2Error::internal(LIST_FAILED))?;
    let Some(time) = v
        .get("create_time")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
    else {
        return Err(V2Error::internal(LIST_FAILED));
    };
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());
    Ok(Some((time, id)))
}

/// The delete endpoints accept a repeated field, a comma-joined one, or both;
/// ids that do not parse are dropped, exactly as the gateway drops them.
pub(crate) fn flatten(values: Vec<String>) -> Vec<Uuid> {
    values
        .iter()
        .flat_map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect()
}
