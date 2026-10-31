use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

pub fn uid(id: &uuid::Uuid) -> String {
    id.to_string()
}

pub fn meta_str(value: &Value) -> String {
    value.to_string()
}

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
