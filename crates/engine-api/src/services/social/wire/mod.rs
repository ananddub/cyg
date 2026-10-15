//! Wire conventions: how the gateway encodes scalars and reads repeated fields.

use chrono::{DateTime, Utc};
use serde_json::Value;

/// REST timestamps drop the sub-second part and the offset: second precision keeps
/// the JSON byte-stable, and the gateway's `AutoSi` produces 0/3/6/9 fractional digits.
pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// Every UUID on the wire is the hyphenated text form; `simple()` would
/// break a client that re-parses it with a UUID regex.
pub fn uid(id: &uuid::Uuid) -> String {
    id.to_string()
}

/// `metadata`, notification `content` and channel-message `content` are `string`s
/// holding JSON, so they are emitted escaped — an object breaks stock SDKs.
pub fn meta_str(value: &Value) -> String {
    value.to_string()
}

/// The gateway carries a repeated field as repeated query params (`?ids=a&ids=b`), one JSON
/// array (`?ids=["a","b"]`, what the JS client writes) or a comma-separated list.
pub fn tokens(raw: Option<&Value>) -> Vec<String> {
    let mut out = Vec::new();
    match raw {
        Some(Value::Array(items)) => {
            out.extend(items.iter().filter_map(Value::as_str).map(str::to_string))
        }
        Some(Value::String(s)) => match serde_json::from_str::<Vec<String>>(s) {
            Ok(parsed) => out.extend(parsed),
            Err(_) => out.push(s.clone()),
        },
        _ => {}
    }
    out.iter()
        .flat_map(|item| {
            item.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}
