use crate::services::deps::q;
use crate::services::quota;

use super::*;

impl super::super::StorageService {
    pub async fn write_objects(
        &self,
        state: &ApiState,
        auth: &Authed,
        mut body: WriteRequest,
    ) -> Result<Json<StorageObjectAcks>, V2Error> {
        let owner = auth.0.uuid()?;
        if body.objects.is_empty() {
            return Ok(Json(StorageObjectAcks { acks: Vec::new() }));
        }
        for object in &mut body.objects {
            if let Ok(parsed) = serde_json::from_str::<Value>(&object.value) {
                object.value = crate::moderation::sanitize_json(parsed).to_string();
            }
            check_write(object)?;
        }
        // A write of many objects spends many units of the window, so one large
        // batch cannot stand in for a thousand small ones.
        quota::charge(
            state,
            owner,
            quota::STORAGE_WRITE,
            body.objects.len() as u32,
        )
        .await?;

        let mut ordered: Vec<(usize, WriteObject)> = body.objects.into_iter().enumerate().collect();
        ordered.sort_by(|a, b| {
            (a.1.collection.as_str(), a.1.key.as_str())
                .cmp(&(b.1.collection.as_str(), b.1.key.as_str()))
        });

        let queries = q(state).await?;
        let mut acks: Vec<Option<StorageObjectAck>> = vec![None; ordered.len()];
        let mut statements: Vec<String> = Vec::new();
        let mut params =
            BTreeMap::<String, Value>::from([("owner".to_string(), json!(owner.to_string()))]);
        for (index, object) in ordered.iter() {
            let current = queries
                .storage_current(&object.collection, &object.key, owner)
                .await
                .map_err(V2Error::from)?;
            let (statement, result) = plan_write(*index, object, current, owner, &mut params)?;
            if let Some(statement) = statement {
                statements.push(statement);
            }
            acks[*index] = Some(StorageObjectAck::for_write(&result, object, owner));
        }

        if !statements.is_empty() {
            queries
                .storage_exec(&statements.join(";\n"), params)
                .await
                .map_err(|err| match err {
                    DbError::Unique(_) => V2Error::failed_precondition(WRITE_REJECTED_MESSAGE),
                    other => V2Error::from(other),
                })?;
        }

        Ok(Json(StorageObjectAcks {
            acks: acks.into_iter().flatten().collect(),
        }))
    }
}

fn plan_write(
    index: usize,
    object: &WriteObject,
    current: Option<StorageCurrentRow>,
    owner: Uuid,
    params: &mut BTreeMap<String, Value>,
) -> Result<(Option<String>, WriteResult), V2Error> {
    let version = version_of(&object.value);
    let read = object.permission_read.unwrap_or(PERMIT_OWNER as i32) as i16;
    let write = object.permission_write.unwrap_or(PERMIT_OWNER as i32) as i16;
    let now = chrono::Utc::now();

    match (object.version.as_str(), current) {
        ("*", Some(_)) => Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE)),
        ("*", None) => Ok((
            Some(storage_insert_statement(index, object, owner, params)),
            inserted(&version, write, now),
        )),
        ("", None) => Ok((
            Some(storage_insert_statement(index, object, owner, params)),
            inserted(&version, write, now),
        )),
        ("", Some(row)) => {
            if row.write != PERMIT_OWNER {
                return Err(V2Error::invalid_argument(WRITE_REJECTED_MESSAGE));
            }
            if row.version == version && row.read == read && row.write == write {
                return Ok((None, unchanged(&row)));
            }
            Ok((
                Some(storage_update_statement(index, object, &version, params)),
                rewritten(&version, write, &row, now),
            ))
        }
        (_, Some(row)) => {
            if row.write != PERMIT_OWNER {
                return Err(V2Error::invalid_argument(WRITE_REJECTED_MESSAGE));
            }
            if row.version != object.version {
                return Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE));
            }
            Ok((
                Some(storage_update_statement(index, object, &version, params)),
                rewritten(&version, write, &row, now),
            ))
        }
        (_, None) => Err(V2Error::failed_precondition(WRITE_REJECTED_MESSAGE)),
    }
}
