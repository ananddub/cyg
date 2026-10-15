//! Pure helpers: channel JSON, stream lookup, wire content/id formatting.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream};

use super::state::channels;

pub(super) fn channel_json(stream: &Stream) -> Value {
    let id = crate::utils::channel::channel_of_stream(
        stream.mode,
        stream.subject,
        stream.descriptor,
        &stream.label,
    );
    let mut obj = json!({ "id": id });
    match stream.mode {
        MODE_DM => {
            obj["user_one_id"] = json!(stream.subject.to_string());
            obj["user_two_id"] = json!(stream.descriptor.to_string());
        }
        MODE_GROUP => obj["group_id"] = json!(stream.subject.to_string()),
        _ => obj["room_name"] = json!(stream.label),
    }
    obj
}

pub(super) fn content_to_string(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(super) fn stream_of(channel_id: &str) -> Option<Stream> {
    channels()
        .read()
        .expect("channel registry lock")
        .get(channel_id)
        .map(|c| c.stream.clone())
}

pub(super) fn uid_simple(id: &Uuid) -> String {
    id.simple().to_string()
}
