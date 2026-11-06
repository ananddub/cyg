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
