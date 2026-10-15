//! `POST /v2/storage` — batch read. An object that does not exist, or that the caller
//! may not read, is simply absent from the answer; the permission check lives in the
//! statement rather than in an error path. The ids travel as parallel arrays joined
//! against `storage`, so the statement keeps a fixed argument count.

use axum::Json;

use crate::dto::storage::{ObjectIdsRequest, StorageObject, StorageObjects};
use crate::utils::storage::{check_id, read_owner, StorageRow};
use super::pool;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::ApiState;

const READ_SQL: &str = "
SELECT storage.collection, storage.key, storage.user_id,
       storage.value::text AS value, storage.version, storage.read, storage.write,
       storage.create_time, storage.update_time
FROM storage
JOIN ROWS FROM (
  unnest($1::varchar[]), unnest($2::varchar[]), unnest($3::uuid[])
) AS wanted(collection, key, user_id)
  ON storage.collection = wanted.collection
 AND storage.key = wanted.key
 AND storage.user_id = wanted.user_id
WHERE (storage.read = 2 OR (storage.read = 1 AND storage.user_id = $4))";

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

        let mut collections = Vec::with_capacity(body.object_ids.len());
        let mut keys = Vec::with_capacity(body.object_ids.len());
        let mut owners = Vec::with_capacity(body.object_ids.len());
        for id in &body.object_ids {
            check_id(id)?;
            collections.push(id.collection.clone());
            keys.push(id.key.clone());
            owners.push(read_owner(id, caller)?);
        }

        let rows = sqlx::query_as::<_, StorageRow>(READ_SQL)
            .bind(&collections)
            .bind(&keys)
            .bind(&owners)
            .bind(caller)
            .fetch_all(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;

        Ok(Json(StorageObjects {
            objects: rows.into_iter().map(StorageObject::from).collect(),
        }))
    }
}
