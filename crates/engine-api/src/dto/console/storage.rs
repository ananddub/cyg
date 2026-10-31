use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::console::ListStorageRow;

#[derive(Debug, Serialize)]
pub struct DeletedCount {
    pub removed: u64,
}

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

impl From<ListStorageRow> for StorageObjectRow {
    fn from(s: ListStorageRow) -> Self {
        Self {
            collection: s.collection,
            key: s.object_key,
            user_id: s.user_id,
            value: serde_json::from_str(&s.value).unwrap_or(serde_json::Value::String(s.value)),
            version: s.version,
            read: s.read,
            write: s.write,
            create_time: s.create_time,
            update_time: s.update_time,
        }
    }
}
