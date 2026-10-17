//! `/v2/storage*` — the `api.StorageObject` shapes and the request bodies.
//!
//! The row → wire mapping lives here; the pure helpers (version hash, request
//! checks, row types) stay in [`crate::utils::storage`].

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::social;
use crate::utils::storage::{StorageRow, WriteResult};

fn is_zero(value: &i32) -> bool {
    *value == 0
}

/// One stored object, as `api.StorageObject` prints it.
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

/// A write's acknowledgement: the object's identity and its new state.
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

/// The gateway drops an empty repeated field, so an
/// empty list is absent rather than `[]`.
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

/// One object in a write batch.
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

/// `PUT /v2/storage` — one batch, applied in one transaction.
#[derive(Debug, Deserialize)]
pub struct WriteRequest {
    #[serde(default)]
    pub objects: Vec<WriteObject>,
}

/// One object in a read or delete batch.
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectId {
    pub collection: String,
    pub key: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub version: String,
}

/// `POST /v2/storage` and `PUT /v2/storage/delete` — the ids named.
#[derive(Debug, Deserialize)]
pub struct ObjectIdsRequest {
    #[serde(default, alias = "objectIds")]
    pub object_ids: Vec<ObjectId>,
}

/// `GET /v2/storage/{collection}[/{user_id}]`'s query.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// `GET /v2/storage/{collection}/{key}/{user_id}`'s query. A version token names the
/// exact state of the object the caller has; a read for a superseded version finds
/// nothing, because the row only ever holds the version it was last written at.
#[derive(Debug, Default, Deserialize)]
pub struct ReadQuery {
    #[serde(default)]
    pub version: Option<String>,
}
