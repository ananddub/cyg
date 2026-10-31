use chrono::{DateTime, SecondsFormat, Utc};

pub fn int64(value: i64) -> Option<String> {
    (value != 0).then(|| value.to_string())
}

pub(crate) fn stamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}
