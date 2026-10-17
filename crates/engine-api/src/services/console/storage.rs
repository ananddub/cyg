//! `/v2/console/storage` — the object browser. Unlike the player storage API,
//! which hides other players' private objects, the console sees every row, and an
//! operator can delete one or clear a whole collection.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::storage::{DeletedCount, StorageObjectRow};
use crate::error::V2Error;
use crate::ApiState;

/// One collection's page; a browser cannot usefully render more.
const MAX_IN_COLLECTION: i64 = 500;

impl super::ConsoleService {
    pub async fn list_storage(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<StorageObjectRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_storage(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(StorageObjectRow::from).collect()))
    }

    pub async fn list_storage_collections(
        &self,
        state: &ApiState,
    ) -> Result<Json<Vec<String>>, V2Error> {
        Ok(Json(super::queries(state)?.list_storage_collections().await?))
    }

    pub async fn list_storage_in_collection(
        &self,
        state: &ApiState,
        collection: &str,
    ) -> Result<Json<Vec<StorageObjectRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_storage_in_collection(collection, MAX_IN_COLLECTION)
            .await?;
        Ok(Json(rows.into_iter().map(StorageObjectRow::from).collect()))
    }

    /// Delete one object. A console delete is unconditional — the operator has seen
    /// the row — but it still names the owner, which is part of the object's key.
    pub async fn delete_storage_object(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        collection: &str,
        key: &str,
        user_id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let user_id = super::account_id(user_id)?;
        if !super::statements::delete_storage_object(super::pool(state)?, collection, key, user_id)
            .await?
        {
            return Err(V2Error::not_found("Object not found."));
        }
        super::audit(
            state,
            claims,
            "storage.object.delete",
            "storage",
            Some(user_id),
            &format!("Deleted {collection}/{key}."),
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Delete every object in a collection. The count is in the reply because
    /// "deleted 0" and "deleted 900" mean very different things to an operator.
    pub async fn delete_storage_collection(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        collection: &str,
    ) -> Result<Json<DeletedCount>, V2Error> {
        // Clearing a collection can take every object any player has stored in it,
        // so it is not shared with the write roles.
        claims.require_admin()?;
        let removed = super::queries(state)?
            .delete_storage_collection(collection)
            .await?
            .rows_affected();
        super::audit(
            state,
            claims,
            "storage.collection.delete",
            "storage",
            None,
            &format!("Deleted {removed} objects from {collection}."),
        )
        .await;
        Ok(Json(DeletedCount { removed }))
    }
}
