use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::V2Error;
use crate::services::social::{cursor_decode, cursor_encode};

use super::{BAD_CURSOR, Stream};

pub(crate) struct ListCursor {
    pub(crate) stream: Stream,
    pub(crate) create_time: DateTime<Utc>,
    pub(crate) id: Uuid,
    pub(crate) forward: bool,
    pub(crate) next: bool,
}

pub(crate) fn encode_cursor(
    stream: &Stream,
    create_time: DateTime<Utc>,
    id: Uuid,
    forward: bool,
    next: bool,
) -> String {
    cursor_encode(&json!({
        "mode": stream.mode,
        "subject": stream.subject.to_string(),
        "descriptor": stream.descriptor.to_string(),
        "label": stream.label,
        "create_time": create_time.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        "id": id.to_string(),
        "forward": forward,
        "is_next": next,
    }))
}

pub(crate) fn decode_cursor(raw: Option<&str>) -> Result<Option<ListCursor>, V2Error> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let v = cursor_decode(raw, BAD_CURSOR)?;
    let read_uuid = |key: &str| -> Result<Uuid, V2Error> {
        match v.get(key).and_then(Value::as_str) {
            Some(s) if s.is_empty() => Ok(Uuid::nil()),
            Some(s) => Uuid::parse_str(s).map_err(|_| V2Error::invalid_argument(BAD_CURSOR)),
            None => Err(V2Error::invalid_argument(BAD_CURSOR)),
        }
    };
    let time = v
        .get("create_time")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| V2Error::invalid_argument(BAD_CURSOR))?;
    let cursor = ListCursor {
        stream: Stream {
            mode: v
                .get("mode")
                .and_then(Value::as_i64)
                .map(|m| m as i16)
                .ok_or_else(|| V2Error::invalid_argument(BAD_CURSOR))?,
            subject: read_uuid("subject")?,
            descriptor: read_uuid("descriptor")?,
            label: v
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        create_time: time,
        id: read_uuid("id")?,
        forward: v.get("forward").and_then(Value::as_bool).unwrap_or(true),
        next: v.get("is_next").and_then(Value::as_bool).unwrap_or(true),
    };
    Ok(Some(cursor))
}
