use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::storage::{DeletedCount, StorageObjectRow};
use crate::error::V2Error;
use crate::services::deps::q;

const MAX_IN_COLLECTION: i64 = 500;

impl super::ConsoleService {
    pub async fn list_storage(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<StorageObjectRow>>, V2Error> {
        let rows = crate::services::games::tenant::list_storage(
            state,
            page.game_id,
            page.limit(),
            page.offset(),
        )
        .await?;
        Ok(Json(rows.into_iter().map(StorageObjectRow::from).collect()))
    }

    pub async fn list_storage_collections(
        &self,
        state: &ApiState,
        game: Option<Uuid>,
    ) -> Result<Json<Vec<String>>, V2Error> {
        Ok(Json(
            crate::services::games::tenant::storage_collections(state, game).await?,
        ))
    }

    pub async fn list_storage_in_collection(
        &self,
        state: &ApiState,
        game: Option<Uuid>,
        collection: &str,
    ) -> Result<Json<Vec<StorageObjectRow>>, V2Error> {
        let rows = crate::services::games::tenant::storage_in_collection(
            state,
            game,
            collection,
            MAX_IN_COLLECTION,
        )
        .await?;
        Ok(Json(rows.into_iter().map(StorageObjectRow::from).collect()))
    }

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
        if !q(state)
            .await?
            .storage_delete_object(collection, key, user_id)
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

    pub async fn delete_storage_collection(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        collection: &str,
    ) -> Result<Json<DeletedCount>, V2Error> {
        claims.require_admin()?;
        let removed =
            crate::services::games::tenant::delete_storage_collection(state, game, collection)
                .await?;
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
