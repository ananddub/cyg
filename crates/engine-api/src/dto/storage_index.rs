use serde::{Deserialize, Serialize};

use crate::dto::storage::StorageObject;

fn is_false(value: &bool) -> bool {
    !*value
}

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

#[derive(Debug, Serialize)]
pub struct StorageIndexDefinitions {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<StorageIndexDef>,
}

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

#[derive(Debug, Serialize)]
pub struct StorageIndexList {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<StorageObject>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub next_cursor: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IndexPageCursor {
    pub query: String,
    pub limit: i32,
    pub order: Vec<String>,
    pub offset: i64,
}
