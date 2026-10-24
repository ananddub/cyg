//! The storage-index plane: definitions live in their own table; every indexed read
//! is a JSONB query over the `storage` rows, permission-filtered like a listing.
//!
//! BaaS parity: `server/storage_index.go` (the index the runtime builds and the
//! console query the server answers) — with the object value as the indexed document.

mod parse;
mod query;
mod sql;
mod statements;

use axum::Json;
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage_index::{StorageIndexDef, StorageIndexDefinitions};
use crate::error::V2Error;

use super::pool;
use statements::{DELETE_INDEX, GET_INDEX, INSERT_INDEX, LIST_INDEXES};

/// One value key is enough to answer with, and four is where a hand-written
/// JSONB path query still earns its name.
pub(super) const MAX_INDEX_FIELDS: usize = 4;
const DEFAULT_MAX_ENTRIES: i32 = 10_000;

pub(super) const MISSING_NAME_MESSAGE: &str = "Storage index name must be set.";
pub(super) const MISSING_COLLECTION_MESSAGE: &str = "Storage index collection must be set.";
pub(super) const MISSING_FIELDS_MESSAGE: &str =
    "Storage index fields must name at least one top-level key of the object value.";
pub(super) const TOO_MANY_FIELDS_MESSAGE: &str =
    "Storage index fields must name at most four top-level keys.";
pub(super) const BAD_MAX_ENTRIES_MESSAGE: &str =
    "Storage index max entries must be greater than zero.";
pub(super) const CLASHED_NAME_MESSAGE: &str =
    "Cannot create index: an index with this name already exists.";
pub(super) const NO_INDEX_MESSAGE: &str = "Index not found.";

/// A stored definition; its `fields` are JSONB because the row holds a list of keys.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct IndexRow {
    pub(super) name: String,
    pub(super) collection: String,
    pub(super) key: String,
    pub(super) fields: Value,
    pub(super) sortable_fields: Value,
    pub(super) max_entries: i32,
    pub(super) index_only: bool,
    pub(super) user_id: Uuid,
}

impl IndexRow {
    /// The JSON arrays as plain lists of key names; anything that is not a string
    /// was never an indexable field and is dropped on the way out.
    pub(super) fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl From<IndexRow> for StorageIndexDef {
    fn from(row: IndexRow) -> Self {
        StorageIndexDef {
            fields: IndexRow::strings(&row.fields),
            sortable_fields: IndexRow::strings(&row.sortable_fields),
            collection: row.collection,
            index: row.name,
            key: row.key,
            max_entries: row.max_entries,
            index_only: row.index_only,
        }
    }
}

/// The rules the runtime applies before it registers a name: a name, a collection,
/// at least one field, and a positive entry cap.
fn validate_create(request: &crate::dto::storage_index::CreateIndexRequest) -> Result<(), V2Error> {
    if request.name.is_empty() {
        return Err(V2Error::invalid_argument(MISSING_NAME_MESSAGE));
    }
    if request.collection.is_empty() {
        return Err(V2Error::invalid_argument(MISSING_COLLECTION_MESSAGE));
    }
    if request.fields.is_empty() {
        return Err(V2Error::invalid_argument(MISSING_FIELDS_MESSAGE));
    }
    if request.fields.len() > MAX_INDEX_FIELDS {
        return Err(V2Error::invalid_argument(TOO_MANY_FIELDS_MESSAGE));
    }
    if request.max_entries.is_some_and(|m| m < 1) {
        return Err(V2Error::invalid_argument(BAD_MAX_ENTRIES_MESSAGE));
    }
    Ok(())
}

impl super::StorageService {
    /// Register a definition; the name is one namespace, so a clash is a conflict.
    pub async fn create_index(
        &self,
        state: &ApiState,
        auth: &Authed,
        request: crate::dto::storage_index::CreateIndexRequest,
    ) -> Result<Json<StorageIndexDef>, V2Error> {
        validate_create(&request)?;
        let max_entries = request.max_entries.unwrap_or(DEFAULT_MAX_ENTRIES);
        let row = sqlx::query_as::<_, IndexRow>(INSERT_INDEX)
            .bind(&request.name)
            .bind(&request.collection)
            .bind(&request.key)
            .bind(serde_json::json!(request.fields))
            .bind(serde_json::json!(request.sortable_fields))
            .bind(max_entries)
            .bind(request.index_only.unwrap_or(false))
            .bind(auth.uid())
            .fetch_optional(pool(state)?)
            .await
            .map_err(|e| {
                if V2Error::is_any_unique_violation(&e) {
                    V2Error::already_exists(CLASHED_NAME_MESSAGE)
                } else {
                    V2Error::from_sqlx(e)
                }
            })?
            .ok_or_else(|| V2Error::internal("Internal server error."))?;
        Ok(Json(row.into()))
    }

    /// Every definition, ordered by name — the runtime's own index listing.
    pub async fn list_indexes(
        &self,
        state: &ApiState,
        _auth: &Authed,
    ) -> Result<Json<StorageIndexDefinitions>, V2Error> {
        let rows = sqlx::query_as::<_, IndexRow>(LIST_INDEXES)
            .fetch_all(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(Json(StorageIndexDefinitions {
            indexes: rows.into_iter().map(StorageIndexDef::from).collect(),
        }))
    }

    /// Drop a definition. Only the player who registered it can take it away, and a
    /// name that no definition holds is the one refusal a retry cannot fix.
    pub async fn delete_index(
        &self,
        state: &ApiState,
        auth: &Authed,
        name: &str,
    ) -> Result<Json<serde_json::Value>, V2Error> {
        let row = self.index_by_name(state, name).await?;
        if row.user_id != auth.uid() {
            return Err(V2Error::permission_denied(
                "Only the player who created this index can delete it.",
            ));
        }
        sqlx::query(DELETE_INDEX)
            .bind(name)
            .execute(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(Json(serde_json::json!({})))
    }

    pub(super) async fn index_by_name(
        &self,
        state: &ApiState,
        name: &str,
    ) -> Result<IndexRow, V2Error> {
        sqlx::query_as::<_, IndexRow>(GET_INDEX)
            .bind(name)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
            .ok_or_else(|| V2Error::not_found(NO_INDEX_MESSAGE))
    }
}
