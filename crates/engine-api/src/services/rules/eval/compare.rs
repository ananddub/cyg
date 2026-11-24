//! Field lookup and comparison helpers for the rule expression tree.

use serde_json::Value;

pub(super) fn lookup<'a>(context: &'a Value, field: &str) -> Option<&'a Value> {
    let mut current = context;
    for part in field.split('.') {
        match current.get(part) {
            Some(next) => current = next,
            None => return None,
        }
    }
    Some(current)
}

pub(super) fn compare(actual: &Value, op: &str, expected: &Value) -> bool {
    match op {
        "eq" => values_equal(actual, expected),
        "ne" => !values_equal(actual, expected),
        // A range is one comparison rather than two joined by `all`, because the
        // graph editor has one box for it and splitting it would make a saved
        // rule unreadable in the editor it came from.
        "between" => expected
            .as_array()
            .is_some_and(|bounds| match bounds.as_slice() {
                [low, high] => in_range(actual, low, high),
                _ => false,
            }),
        // Days since a timestamp field, so a rule can say "logged in this week"
        // without the graph needing arithmetic.
        "within_days" => {
            let days = expected.as_f64().unwrap_or(0.0);
            days_since(actual).is_some_and(|age| age <= days)
        }
        "older_than_days" => {
            let days = expected.as_f64().unwrap_or(0.0);
            days_since(actual).is_some_and(|age| age > days)
        }
        "in" => expected
            .as_array()
            .is_some_and(|items| items.iter().any(|item| values_equal(actual, item))),
        "contains" => match actual {
            Value::Array(items) => items.iter().any(|item| values_equal(item, expected)),
            Value::String(text) => expected
                .as_str()
                .is_some_and(|needle| text.contains(needle)),
            _ => false,
        },
        ordering => match (as_number(actual), as_number(expected)) {
            (Some(left), Some(right)) => match ordering {
                "gt" => left > right,
                "gte" => left >= right,
                "lt" => left < right,
                "lte" => left <= right,
                _ => false,
            },
            _ => match (actual.as_str(), expected.as_str()) {
                (Some(left), Some(right)) => match ordering {
                    "gt" => left > right,
                    "gte" => left >= right,
                    "lt" => left < right,
                    "lte" => left <= right,
                    _ => false,
                },
                _ => false,
            },
        },
    }
}

fn in_range(actual: &Value, low: &Value, high: &Value) -> bool {
    match (as_number(actual), as_number(low), as_number(high)) {
        (Some(value), Some(low), Some(high)) => value >= low && value <= high,
        _ => match (actual.as_str(), low.as_str(), high.as_str()) {
            (Some(value), Some(low), Some(high)) => value >= low && value <= high,
            _ => false,
        },
    }
}

/// Whole days between a timestamp field and now.
///
/// Accepts both a unix epoch number and an RFC 3339 string, because a stat map
/// written by a game and one written by the account service do not agree on the
/// shape. A field that is neither is not a date, so the comparison is false
/// rather than an error — same rule as every other missing field.
fn days_since(value: &Value) -> Option<f64> {
    let seconds = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text).ok()?.timestamp() as f64,
        _ => return None,
    };
    // A timestamp beyond this century is more likely milliseconds than seconds.
    let seconds = if seconds > 32_503_680_000.0 {
        seconds / 1000.0
    } else {
        seconds
    };
    let now = chrono::Utc::now().timestamp() as f64;
    Some(((now - seconds) / 86_400.0).max(0.0))
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (as_number(left), as_number(right)) {
        (Some(a), Some(b)) => (a - b).abs() < f64::EPSILON,
        _ => left == right,
    }
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}
