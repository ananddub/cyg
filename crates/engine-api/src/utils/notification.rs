use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::V2Error;
use crate::repositories::models::Notification;
use crate::services::social::{cursor_decode, cursor_encode, uid};

pub(crate) const LIMIT_RANGE: &str = "Invalid limit - limit must be between 1 and 100.";
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

pub(crate) fn flatten(values: Vec<String>) -> Vec<Uuid> {
    values
        .iter()
        .flat_map(|v| {
            let raw = v.trim();
            if raw.starts_with('[') {
                if let Ok(items) = serde_json::from_str::<Vec<String>>(raw) {
                    return items;
                }
            }
            raw.split(',').map(str::trim).map(str::to_owned).collect()
        })
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect()
}
