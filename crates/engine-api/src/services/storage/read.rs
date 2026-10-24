//! `POST /v2/storage` — batch read. An object that does not exist, or that the caller
//! may not read, is simply absent from the answer; the permission check lives in the
//! statement rather than in an error path. The ids travel as parallel arrays joined
//! against `storage`, so the statement keeps a fixed argument count.

use axum::Json;
use uuid::Uuid;

use super::pool;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage::{ObjectIdsRequest, ReadQuery, StorageObject, StorageObjects};
use crate::error::V2Error;
use crate::utils::storage::{
    INVALID_OBJECT_MESSAGE, INVALID_USER_MESSAGE, StorageRow, check_id, read_owner,
};

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

/// A single object, addressed by its full key. The permission predicate is the
/// batch read's, moved from a join to a `WHERE`; the version token is optional, so
/// the caller may read the object as it stands or only if it still carries a version
/// they already hold.
const READ_ONE_SQL: &str = "
SELECT collection, key, user_id,
       value::text AS value, version, read, write,
       create_time, update_time
FROM storage
WHERE collection = $1 AND key = $2 AND user_id = $3
  AND (read = 2 OR (read = 1 AND user_id = $4))
  AND ($5::varchar IS NULL OR version = $5)";

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

    /// `GET /v2/storage/{collection}/{key}/{user_id}` — one object. The all-zero
    /// owner names the caller's own account, which is how the REST form asks for
    /// "my object"; an object the caller may not read is reported as absent, the
    /// way the batch read hides it, so the route answers a lookup and nothing else.
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

        let row = sqlx::query_as::<_, StorageRow>(READ_ONE_SQL)
            .bind(collection)
            .bind(key)
            .bind(owner)
            .bind(caller)
            .bind(version.as_deref())
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
            .ok_or_else(|| V2Error::not_found("Object not found."))?;
        Ok(Json(StorageObject::from(row)))
    }
}
