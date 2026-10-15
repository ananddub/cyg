//! `GET /v2/storage/{collection}[/{user_id}]` — listing.
//!
//! Which rows a caller may page through depends on who they are and whom they ask about:
//! anyone's public objects, the caller's own (private included), or one other user's public.

mod cursor;
mod fetch;
mod reply;
mod statements;

use axum::Json;
use uuid::Uuid;

use crate::dto::storage::{ListQuery, StorageObjectList};
use crate::utils::storage::*;
use super::pool;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::services::social;
use crate::ApiState;

use cursor::resume;
use fetch::{fetch, Listing};
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
        let rows = fetch(
            pool(state)?,
            Listing::PublicAll,
            collection,
            None,
            after.as_ref(),
            limit,
        )
        .await?;
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
        let owner =
            Uuid::parse_str(user_id).map_err(|_| V2Error::invalid_argument(INVALID_USER_MESSAGE))?;
        let listing = if owner == auth.0.uuid()? {
            Listing::Own
        } else {
            Listing::PublicUser
        };
        let limit = social::limit(req.limit, 1, MAX_LIST_LIMIT, LIST_LIMIT_MESSAGE)?;
        let after = resume(req.cursor.as_ref())?;
        let rows = fetch(
            pool(state)?,
            listing,
            collection,
            Some(owner),
            after.as_ref(),
            limit,
        )
        .await?;
        Ok(reply(rows, limit, req.cursor.as_deref().unwrap_or("")))
    }
}
