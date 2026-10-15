//! `PUT /v2/storage/delete` — batch delete in one transaction, in a stable collection/key
//! order so two clients deleting the same set cannot deadlock. A client deletes its own
//! objects and nothing else, and one refusal rolls the whole batch back.

use axum::Json;

use crate::dto::storage::ObjectIdsRequest;
use crate::dto::Empty;
use crate::utils::storage::{check_id, DELETE_REJECTED_MESSAGE};
use super::pool;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

const DELETE_SQL: &str =
    "DELETE FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3 AND write > 0";

const DELETE_VERSION_SQL: &str = "DELETE FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3 AND write > 0 AND version = $4";

impl super::StorageService {
    pub async fn delete_objects(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: ObjectIdsRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let mut ids = body.object_ids;
        if ids.is_empty() {
            return Ok(Json(Empty {}));
        }
        for id in &ids {
            check_id(id)?;
        }
        ids.sort_by(|a, b| {
            (a.collection.as_str(), a.key.as_str()).cmp(&(b.collection.as_str(), b.key.as_str()))
        });

        let owner = auth.0.uuid()?;
        let db = pool(state)?;
        let mut tx = db.begin().await?;
        for id in &ids {
            let mut query = sqlx::query(if id.version.is_empty() {
                DELETE_SQL
            } else {
                DELETE_VERSION_SQL
            })
            .bind(&id.collection)
            .bind(&id.key)
            .bind(owner);
            if !id.version.is_empty() {
                query = query.bind(&id.version);
            }
            let removed = query.execute(&mut *tx).await.map_err(V2Error::from_sqlx)?;
            if removed.rows_affected() == 0 {
                return Err(V2Error::invalid_argument(DELETE_REJECTED_MESSAGE));
            }
        }
        tx.commit().await?;

        Ok(Json(Empty {}))
    }
}
