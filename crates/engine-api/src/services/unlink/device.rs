//! The two unlinks that detach a row from `user_device` rather than a column from `users`.
//!
//! Both are a pair of statements — the guarded delete, then a touch that keeps `update_time`
//! honest — and the only difference is whether that touch may match nothing.

use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use super::bearer;
use crate::dto::Empty;
use crate::utils::unlink::{refused};
use crate::ApiState;
use crate::repositories::db::Queries;
use crate::services::auth::q;
use crate::dto::auth;
use crate::error::V2Error;

impl super::UnlinkService {
    /// `POST /v2/account/unlink/device`. Refused when it would leave the account with
    /// nothing to sign in with, so a device-only account cannot unlink it.
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
        let queries = q(state)?;
        queries
            .unlink_device(&account.id, uid)
            .await
            .map_err(refused(FAILED))?;
        queries.touch_user(uid).await.map_err(refused(FAILED))?;
        Ok(Json(Empty {}))
    }

    /// `POST /v2/account/unlink`. Detaching every device row under a runtime provider's
    /// name; rows are keyed by its lowercase form, so an unmatched case still finds them.
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
        let queries = q(state)?;
        queries
            .unlink_provider(&name, uid)
            .await
            .map_err(refused(FAILED))?;
        touch_ignoring_absent(&queries, uid, FAILED).await?;
        Ok(Json(Empty {}))
    }
}

/// The touch this route does not read a row count from, so an account that went away
/// between the two statements is still unlinked (a database fault is logged, not described).
async fn touch_ignoring_absent(
    queries: &Queries,
    uid: Uuid,
    failed: &'static str,
) -> Result<(), V2Error> {
    if let Err(e) = queries.touch_user(uid).await {
        if !matches!(e, sqlx::Error::RowNotFound) {
            eprintln!("[v2] {failed}: {e}");
            return Err(V2Error::internal(failed));
        }
    }
    Ok(())
}
