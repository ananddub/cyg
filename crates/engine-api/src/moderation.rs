use rustrict::{Censor, Type};

pub fn is_clean(text: &str) -> bool {
    !Censor::from_str(text)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
}

pub fn clean_text(text: &str) -> String {
    Censor::from_str(text)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .censor()
}

pub fn sanitize_json(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(clean_text(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize_json).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, sanitize_json(v)))
                .collect(),
        ),
        other => other,
    }
}

pub fn is_valid_username(name: &str) -> bool {
    let len = name.trim().len();
    len >= 3 && len <= 20 && is_clean(name)
}
