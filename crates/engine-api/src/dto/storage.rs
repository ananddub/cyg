use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::storage::StorageRow;
use crate::services::social;
use crate::utils::storage::WriteResult;

fn is_zero(value: &i32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageObject {
    pub collection: String,
    pub key: String,
    pub user_id: String,
    pub value: String,
    pub version: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub permission_read: i32,
    #[serde(skip_serializing_if = "is_zero")]
    pub permission_write: i32,
    pub create_time: String,
    pub update_time: String,
}

impl From<StorageRow> for StorageObject {
    fn from(row: StorageRow) -> Self {
        StorageObject {
            collection: row.collection,
            key: row.key,
            user_id: social::uid(&row.user_id),
            value: row.value,
            version: row.version,
            permission_read: row.read as i32,
            permission_write: row.write as i32,
            create_time: social::ts(row.create_time),
            update_time: social::ts(row.update_time),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageObjectAck {
    pub collection: String,
    pub key: String,
    pub version: String,
    pub user_id: String,
    pub create_time: String,
    pub update_time: String,
}

impl StorageObjectAck {
    pub fn for_write(result: &WriteResult, object: &WriteObject, owner: Uuid) -> Self {
        StorageObjectAck {
            collection: object.collection.clone(),
            key: object.key.clone(),
            version: result.version.clone(),
            user_id: social::uid(&owner),
            create_time: social::ts(result.create_time),
            update_time: social::ts(result.update_time),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StorageObjects {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<StorageObject>,
}

#[derive(Debug, Serialize)]
pub struct StorageObjectAcks {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub acks: Vec<StorageObjectAck>,
}

#[derive(Debug, Serialize)]
pub struct StorageObjectList {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<StorageObject>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cursor: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WriteObject {
    pub collection: String,
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, alias = "permissionRead")]
    pub permission_read: Option<i32>,
    #[serde(default, alias = "permissionWrite")]
    pub permission_write: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct WriteRequest {
    #[serde(default)]
    pub objects: Vec<WriteObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectId {
    pub collection: String,
    pub key: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct ObjectIdsRequest {
    #[serde(default, alias = "objectIds")]
    pub object_ids: Vec<ObjectId>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReadQuery {
    #[serde(default)]
    pub version: Option<String>,
}
