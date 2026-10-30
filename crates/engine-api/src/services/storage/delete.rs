use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::dto::storage::ObjectIdsRequest;
use crate::error::V2Error;
use crate::services::deps::q;
use crate::utils::storage::{DELETE_REJECTED_MESSAGE, check_id};

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
        let queries = q(state).await?;
        let rows = queries
            .storage_deletable(owner, &ids)
            .await
            .map_err(V2Error::from)?;

        for id in &ids {
            let deletable = rows.iter().any(|row| {
                row.collection == id.collection
                    && row.key == id.key
                    && row.write > 0
                    && (id.version.is_empty() || row.version == id.version)
            });
            if !deletable {
                return Err(V2Error::invalid_argument(DELETE_REJECTED_MESSAGE));
            }
        }

        queries
            .storage_delete(owner, &ids)
            .await
            .map_err(V2Error::from)?;

        Ok(Json(Empty {}))
    }
}
