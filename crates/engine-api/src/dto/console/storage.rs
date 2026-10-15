//! The storage browser: an object as the console shows it, with its value parsed
//! back into JSON so the viewer can pretty-print it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::console::ConsoleStorageRow;

/// One stored object. `read`/`write` are the permission levels the storage API
/// numbers (0 private, 1 owner, 2 public), kept numeric here for the editor.
#[derive(Debug, Serialize)]
pub struct StorageObjectRow {
    pub collection: String,
    pub key: String,
    pub user_id: Uuid,
    pub value: serde_json::Value,
    pub version: String,
    pub read: i16,
    pub write: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<ConsoleStorageRow> for StorageObjectRow {
    fn from(s: ConsoleStorageRow) -> Self {
        Self {
            collection: s.collection,
            key: s.key,
            user_id: s.user_id,
            // `value` is selected as text; a column that somehow is not JSON shows
            // as a JSON string rather than failing the whole listing.
            value: serde_json::from_str(&s.value).unwrap_or(serde_json::Value::String(s.value)),
            version: s.version,
            read: s.read,
            write: s.write,
            create_time: s.create_time,
            update_time: s.update_time,
        }
    }
}
