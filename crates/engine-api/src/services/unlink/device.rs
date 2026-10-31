use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use super::bearer;
use crate::ApiState;
use crate::db::DbError;
use crate::dto::Empty;
use crate::dto::auth;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::deps::q;
use crate::utils::unlink::refused;

impl super::UnlinkService {
    pub async fn device(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        const FAILED: &str = "Could not unlink device ID.";
        let uid = bearer(headers, state)?;
        if account.id.is_empty() {
            return Err(V2Error::invalid_argument("A device ID must be supplied."));
        }
        let queries = q(state).await?;
        queries
            .unlink_device(&account.id, uid)
            .await
            .map_err(refused(FAILED))?;
        queries.touch_user(uid).await.map_err(refused(FAILED))?;
        Ok(Json(Empty {}))
    }

    pub async fn provider(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::ProviderAccount,
    ) -> Result<Json<Empty>, V2Error> {
        const FAILED: &str = "Error while trying to unlink provider identity.";
        let uid = bearer(headers, state)?;
        if account.provider.is_empty() {
            return Err(V2Error::invalid_argument(
                "A provider name must be supplied.",
            ));
        }
        let name = account.provider.to_lowercase();
        let queries = q(state).await?;
        queries
            .unlink_provider(&name, uid)
            .await
            .map_err(refused(FAILED))?;
        touch_ignoring_absent(&queries, uid, FAILED).await?;
        Ok(Json(Empty {}))
    }
}

async fn touch_ignoring_absent(
    queries: &Queries,
    uid: Uuid,
    failed: &'static str,
) -> Result<(), V2Error> {
    if let Err(e) = queries.touch_user(uid).await {
        if !matches!(e, DbError::NotFound) {
            tracing::error!("[v2] {failed}: {e}");
            return Err(V2Error::internal(failed));
        }
    }
    Ok(())
}
