use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage::{ObjectIdsRequest, ReadQuery, StorageObject, StorageObjects};
use crate::error::V2Error;
use crate::services::deps::q;
use crate::utils::storage::{INVALID_OBJECT_MESSAGE, INVALID_USER_MESSAGE, check_id, read_owner};

impl super::StorageService {
    pub async fn read_objects(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: ObjectIdsRequest,
    ) -> Result<Json<StorageObjects>, V2Error> {
        let caller = auth.0.uuid()?;
        if body.object_ids.is_empty() {
            return Ok(Json(StorageObjects {
                objects: Vec::new(),
            }));
        }

        let mut wanted: Vec<(String, String, Uuid)> = Vec::with_capacity(body.object_ids.len());
        for id in &body.object_ids {
            check_id(id)?;
            wanted.push((
                id.collection.clone(),
                id.key.clone(),
                read_owner(id, caller)?,
            ));
        }

        let queries = q(state).await?;
        let mut rows = queries
            .storage_objects(caller, &wanted)
            .await
            .map_err(V2Error::from)?;
        rows.sort_by_key(|row| {
            wanted
                .iter()
                .position(|(collection, key, owner)| {
                    *collection == row.collection && *key == row.key && *owner == row.user_id
                })
                .unwrap_or(usize::MAX)
        });

        Ok(Json(StorageObjects {
            objects: rows.into_iter().map(StorageObject::from).collect(),
        }))
    }

    pub async fn read_object(
        &self,
        state: &ApiState,
        auth: &Authed,
        collection: &str,
        key: &str,
        user_id: &str,
        req: ReadQuery,
    ) -> Result<Json<StorageObject>, V2Error> {
        if collection.is_empty() || key.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_OBJECT_MESSAGE));
        }
        let caller = auth.0.uuid()?;
        let parsed = Uuid::parse_str(user_id)
            .map_err(|_| V2Error::invalid_argument(INVALID_USER_MESSAGE))?;
        let owner = if parsed == Uuid::nil() {
            caller
        } else {
            parsed
        };
        let version = req.version.filter(|v| !v.is_empty());

        let row = q(state)
            .await?
            .storage_object(caller, collection, key, owner, version.as_deref())
            .await
            .map_err(V2Error::from)?
            .ok_or_else(|| V2Error::not_found("Object not found."))?;
        Ok(Json(StorageObject::from(row)))
    }
}
