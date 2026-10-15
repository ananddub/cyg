//! The printing rules the record shapes use: an int64 as a decimal
//! string (absent when zero), and a timestamp from `Unix()` seconds only, so nanos never print.

use chrono::{DateTime, SecondsFormat, Utc};

/// The gateway prints int64 and uint64 as strings, and omits a zero outright.
pub fn int64(value: i64) -> Option<String> {
    (value != 0).then(|| value.to_string())
}

/// BaaS fills a timestamp from `Unix()` seconds only, so nanos never print.
pub(crate) fn stamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}
