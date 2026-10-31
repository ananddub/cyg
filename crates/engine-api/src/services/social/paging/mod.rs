use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::error::V2Error;

pub fn position_now() -> i64 {
    Utc::now().timestamp_nanos_opt().unwrap_or_default().max(0)
}

pub fn cursor_encode(value: &Value) -> String {
    URL_SAFE_NO_PAD.encode(value.to_string().as_bytes())
}

pub fn cursor_decode(raw: &str, message: &str) -> Result<Value, V2Error> {
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw.as_bytes())
        .map_err(|_| V2Error::invalid_argument(message))?;
    serde_json::from_slice(&bytes).map_err(|_| V2Error::invalid_argument(message))
}

pub fn cursor_i64(cursor: &Value, key: &str, default: i64) -> i64 {
    cursor.get(key).and_then(Value::as_i64).unwrap_or(default)
}

pub fn cursor_uuid(cursor: &Value, key: &str) -> Uuid {
    cursor
        .get(key)
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Uuid::nil)
}

pub fn cursor_time(cursor: &Value, key: &str) -> DateTime<Utc> {
    cursor
        .get(key)
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or(ts_head())
}

pub fn ts_head() -> DateTime<Utc> {
    Utc.timestamp_opt(-62_135_596_800, 0).unwrap()
}

pub fn ts_tail() -> DateTime<Utc> {
    Utc.timestamp_opt(253_402_300_799, 999_999_000).unwrap()
}

pub fn limit(
    requested: Option<i64>,
    default: i64,
    max: i64,
    message: impl Into<String>,
) -> Result<i64, V2Error> {
    match requested {
        None => Ok(default),
        Some(n) if (1..=max).contains(&n) => Ok(n),
        Some(_) => Err(V2Error::invalid_argument(message)),
    }
}
