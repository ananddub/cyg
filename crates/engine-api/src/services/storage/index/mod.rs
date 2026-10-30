mod order;
mod parse;
mod query;

use axum::Json;

use crate::services::deps::q;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::storage_index::{StorageIndexDef, StorageIndexDefinitions};
use crate::error::V2Error;
use crate::repositories::storage::{StorageIndexContent, StorageIndexRow};

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
    pub async fn create_index(
        &self,
        state: &ApiState,
        auth: &Authed,
        request: crate::dto::storage_index::CreateIndexRequest,
    ) -> Result<Json<StorageIndexDef>, V2Error> {
        validate_create(&request)?;
        let content = StorageIndexContent {
            name: request.name,
            collection: request.collection,
            key: request.key,
            fields: request.fields,
            sortable_fields: request.sortable_fields,
            max_entries: request.max_entries.unwrap_or(DEFAULT_MAX_ENTRIES),
            index_only: request.index_only.unwrap_or(false),
            user_id: auth.uid(),
            create_time: chrono::Utc::now(),
        };
        let row = q(state).await?
            .storage_insert_index(&content)
            .await
            .map_err(|err| match &err {
                DbError::Unique(_) => V2Error::already_exists(CLASHED_NAME_MESSAGE),
                DbError::Query(message) if message.contains("already exists") => {
                    V2Error::already_exists(CLASHED_NAME_MESSAGE)
                }
                other => V2Error::from(other.clone()),
            })?;
        row.map(|row| Json(row.into()))
            .ok_or_else(|| V2Error::internal("Internal server error."))
    }

    pub async fn list_indexes(
        &self,
        state: &ApiState,
        _auth: &Authed,
    ) -> Result<Json<StorageIndexDefinitions>, V2Error> {
        let rows = q(state).await?
            .storage_indexes()
            .await
            .map_err(V2Error::from)?;
        Ok(Json(StorageIndexDefinitions {
            indexes: rows.into_iter().map(StorageIndexDef::from).collect(),
        }))
    }

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
        q(state).await?
            .storage_delete_index(name)
            .await
            .map_err(V2Error::from)?;
        Ok(Json(serde_json::json!({})))
    }

    pub(super) async fn index_by_name(
        &self,
        state: &ApiState,
        name: &str,
    ) -> Result<StorageIndexRow, V2Error> {
        q(state).await?
            .storage_index(name)
            .await
            .map_err(V2Error::from)?
            .ok_or_else(|| V2Error::not_found(NO_INDEX_MESSAGE))
    }
}
