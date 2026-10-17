//! `/v2/storage/index*` — the storage-index wire shapes.
//!
//! The definition fields keep the reference server's names (`name`, `collection`, `key`, `fields`,
//! `sortable_fields`, `max_entries`, `index_only`); the query body keeps `query`,
//! `limit`, `order` and `cursor`. The answer reuses `dto::storage::StorageObject`,
//! because an indexed read returns the same objects a direct read does.

use serde::{Deserialize, Serialize};

use crate::dto::storage::StorageObject;

fn is_false(value: &bool) -> bool {
    !*value
}

/// `POST /v2/storage/index` — define an index over one collection.
#[derive(Debug, Deserialize)]
pub struct CreateIndexRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub collection: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default, alias = "sortableFields")]
    pub sortable_fields: Vec<String>,
    #[serde(default, alias = "maxEntries")]
    pub max_entries: Option<i32>,
    #[serde(default, alias = "indexOnly")]
    pub index_only: Option<bool>,
}

/// One index, printed the way the index list prints it: `index` names it.
#[derive(Debug, Serialize)]
pub struct StorageIndexDef {
    pub collection: String,
    pub index: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sortable_fields: Vec<String>,
    pub max_entries: i32,
    #[serde(skip_serializing_if = "is_false")]
    pub index_only: bool,
}

/// `GET /v2/storage/index` — every definition, by name.
#[derive(Debug, Serialize)]
pub struct StorageIndexDefinitions {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<StorageIndexDef>,
}

/// `POST /v2/storage/index/{name}/query` — the request the reference server's console query sends:
/// a query string, a page size, a sort, and the cursor from the last page.
#[derive(Debug, Deserialize)]
pub struct IndexQueryRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub limit: Option<i32>,
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub cursor: String,
}

/// The page of objects an index matched.
#[derive(Debug, Serialize)]
pub struct StorageIndexList {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<StorageObject>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub next_cursor: String,
}

/// The cursor's contents, mirrored from the index list it came from: a cursor is
/// only valid for the same query, limit and order that produced it.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IndexPageCursor {
    pub query: String,
    pub limit: i32,
    pub order: Vec<String>,
    pub offset: i64,
}
