mod cursor;
mod reply;

use axum::Json;
use uuid::Uuid;

use crate::services::deps::q;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage::{ListQuery, StorageObjectList};
use crate::error::V2Error;
use crate::repositories::storage::StorageListing;
use crate::services::social;
use crate::utils::storage::*;

use cursor::resume;
use reply::reply;

impl super::StorageService {
    pub async fn list_collection(
        &self,
        state: &ApiState,
        _auth: &Authed,
        collection: &str,
        req: ListQuery,
    ) -> Result<Json<StorageObjectList>, V2Error> {
        if collection.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_OBJECT_MESSAGE));
        }
        let limit = social::limit(req.limit, 1, MAX_LIST_LIMIT, LIST_LIMIT_MESSAGE)?;
        let after = resume(req.cursor.as_ref())?;
        let rows = q(state).await?
            .storage_listing(
                StorageListing::PublicAll,
                collection,
                None,
                after.as_ref(),
                limit,
            )
            .await
            .map_err(V2Error::from)?;
        Ok(reply(rows, limit, req.cursor.as_deref().unwrap_or("")))
    }

    pub async fn list_user_objects(
        &self,
        state: &ApiState,
        auth: &Authed,
        collection: &str,
        user_id: &str,
        req: ListQuery,
    ) -> Result<Json<StorageObjectList>, V2Error> {
        if collection.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_OBJECT_MESSAGE));
        }
        let owner = Uuid::parse_str(user_id)
            .map_err(|_| V2Error::invalid_argument(INVALID_USER_MESSAGE))?;
        let listing = if owner == auth.0.uuid()? {
            StorageListing::Own
        } else {
            StorageListing::PublicUser
        };
        let limit = social::limit(req.limit, 1, MAX_LIST_LIMIT, LIST_LIMIT_MESSAGE)?;
        let after = resume(req.cursor.as_ref())?;
        let rows = q(state).await?
            .storage_listing(listing, collection, Some(owner), after.as_ref(), limit)
            .await
            .map_err(V2Error::from)?;
        Ok(reply(rows, limit, req.cursor.as_deref().unwrap_or("")))
    }
}
