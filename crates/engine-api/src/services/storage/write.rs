//! `PUT /v2/storage` — batch write with the client-driven version token.

use axum::Json;
use uuid::Uuid;

use super::pool;
use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage::{StorageObjectAck, StorageObjectAcks, WriteObject, WriteRequest};
use crate::error::V2Error;
use crate::utils::storage::*;

const IF_MATCH_SQL: &str = "
WITH upd AS (
    UPDATE storage SET value = $4::jsonb, version = $5, read = $6, write = $7, update_time = now()
    WHERE collection = $1 AND key = $2 AND user_id = $3 AND version = $8 AND storage.write = 1
    RETURNING write, version, create_time, update_time
)
(SELECT write, version, create_time, update_time, true AS applied FROM upd)
UNION ALL
(SELECT write, version, create_time, update_time, false AS applied
 FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3 AND NOT EXISTS (SELECT 1 FROM upd))
LIMIT 1";

const UPSERT_SQL: &str = "
WITH upd AS (
    INSERT INTO storage (collection, key, user_id, value, version, read, write, create_time, update_time)
    VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, now(), now())
    ON CONFLICT (collection, key, user_id) DO
        UPDATE SET value = $4::jsonb, version = $5, read = $6, write = $7, update_time = now()
        WHERE storage.write = 1 AND NOT (storage.version = $5 AND storage.read = $6 AND storage.write = $7)
    RETURNING write, version, create_time, update_time
)
(SELECT write, version, create_time, update_time, true AS applied FROM upd)
UNION ALL
(SELECT write, version, create_time, update_time, false AS applied
 FROM storage WHERE collection = $1 AND key = $2 AND user_id = $3 AND NOT EXISTS (SELECT 1 FROM upd))
LIMIT 1";

const INSERT_SQL: &str = "
INSERT INTO storage (collection, key, user_id, value, version, read, write, create_time, update_time)
VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, now(), now())
RETURNING write, version, create_time, update_time, true AS applied";

impl super::StorageService {
    /// Writes run in one transaction and in a stable key order: a rejected object
    /// aborts the batch, so a client never sees half a write succeed.
    pub async fn write_objects(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: WriteRequest,
    ) -> Result<Json<StorageObjectAcks>, V2Error> {
        let owner = auth.0.uuid()?;
        if body.objects.is_empty() {
            return Ok(Json(StorageObjectAcks { acks: Vec::new() }));
        }
        for object in &body.objects {
            check_write(object)?;
        }

        let mut ordered: Vec<(usize, WriteObject)> = body.objects.into_iter().enumerate().collect();
        ordered.sort_by(|a, b| {
            (a.1.collection.as_str(), a.1.key.as_str())
                .cmp(&(b.1.collection.as_str(), b.1.key.as_str()))
        });

        let db = pool(state)?;
        let mut tx = db.begin().await?;
        let mut acks: Vec<Option<StorageObjectAck>> = vec![None; ordered.len()];
        for (index, object) in ordered {
            let ack = apply_write(&mut tx, owner, &object).await?;
            acks[index] = Some(ack);
        }
        tx.commit().await?;

        Ok(Json(StorageObjectAcks {
            acks: acks.into_iter().flatten().collect(),
        }))
    }
}

async fn apply_write(
    tx: &mut sqlx::PgConnection,
    owner: Uuid,
    object: &WriteObject,
) -> Result<StorageObjectAck, V2Error> {
    let version = version_of(&object.value);
    let read = object.permission_read.unwrap_or(PERMIT_OWNER as i32);
    let write = object.permission_write.unwrap_or(PERMIT_OWNER as i32);

    let result = match object.version.as_str() {
        "" => run(tx, UPSERT_SQL, object, owner, &version, read, write, None)
            .await
            .map_err(V2Error::from_sqlx)?,
        "*" => match run(tx, INSERT_SQL, object, owner, &version, read, write, None).await {
            Ok(result) => result,
            Err(e) if V2Error::is_unique_violation(&e, "storage_pkey") => {
                return Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE));
            }
            Err(e) => return Err(V2Error::from_sqlx(e)),
        },
        expected => run(
            tx,
            IF_MATCH_SQL,
            object,
            owner,
            &version,
            read,
            write,
            Some(expected),
        )
        .await
        .map_err(V2Error::from_sqlx)?,
    };

    let Some(result) = result else {
        return Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE));
    };
    if !result.applied {
        if result.write != PERMIT_OWNER {
            return Err(V2Error::invalid_argument(WRITE_REJECTED_MESSAGE));
        }
        if !object.version.is_empty() {
            return Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE));
        }
    }
    Ok(StorageObjectAck::for_write(&result, object, owner))
}

async fn run(
    tx: &mut sqlx::PgConnection,
    sql: &str,
    object: &WriteObject,
    owner: Uuid,
    version: &str,
    read: i32,
    write: i32,
    expected: Option<&str>,
) -> Result<Option<WriteResult>, sqlx::Error> {
    let mut query = sqlx::query_as::<_, WriteResult>(sql)
        .bind(&object.collection)
        .bind(&object.key)
        .bind(owner)
        .bind(&object.value)
        .bind(version)
        .bind(read as i16)
        .bind(write as i16);
    if let Some(expected) = expected {
        query = query.bind(expected);
    }
    query.fetch_optional(tx).await
}
