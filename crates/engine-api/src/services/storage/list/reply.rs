use axum::Json;
use serde_json::json;

use crate::dto::storage::{StorageObject, StorageObjectList};
use crate::repositories::storage::StorageRow;
use crate::services::social;

pub(super) fn reply(rows: Vec<StorageRow>, limit: i64, sent: &str) -> Json<StorageObjectList> {
    let mut objects: Vec<StorageObject> = rows.into_iter().map(StorageObject::from).collect();
    let mut cursor = String::new();
    if objects.len() as i64 > limit {
        objects.truncate(limit as usize);
        if let Some(last) = objects.last() {
            cursor = social::cursor_encode(&json!({
                "key": last.key,
                "read": last.permission_read,
                "user_id": last.user_id,
            }));
        }
    }
    if cursor == sent {
        cursor.clear();
    }
    Json(StorageObjectList { objects, cursor })
}
