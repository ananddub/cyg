use rustrict::{Censor, Type};

/// Checks whether the provided text contains any inappropriate content or profanity.
/// Returns `true` if the text is clean and acceptable.
pub fn is_clean(text: &str) -> bool {
    !Censor::from_str(text)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
}

/// Automatically censors profane words with asterisks and returns the sanitized text.
pub fn clean_text(text: &str) -> String {
    Censor::from_str(text)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .censor()
}

/// Recursively sanitizes every string value in a chat message's JSON content
/// (object keys untouched, string values censored with asterisks). Wired into
/// the channel message pipeline so no player-authored text reaches storage or
/// other members unfiltered.
pub fn sanitize_json(value: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(clean_text(&text)),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(sanitize_json).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter().map(|(k, v)| (k, sanitize_json(v))).collect(),
        ),
        other => other,
    }
}

/// Validates player usernames:
/// - Must be between 3 and 20 characters in length
/// - Must not contain profanity or offensive terms
pub fn is_valid_username(name: &str) -> bool {
    let len = name.trim().len();
    len >= 3 && len <= 20 && is_clean(name)
}
