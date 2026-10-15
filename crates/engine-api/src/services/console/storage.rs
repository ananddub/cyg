//! `/v2/console/storage` — the object browser. Unlike the player storage API,
//! which hides other players' private objects, the console sees every row.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::storage::StorageObjectRow;
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
}
