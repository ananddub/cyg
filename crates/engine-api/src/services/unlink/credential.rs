use axum::Json;
use axum::http::HeaderMap;

use super::bearer;
use crate::dto::Empty;
use crate::dto::auth;
use crate::error::V2Error;
use crate::utils::unlink::{last_account, refused};

impl super::UnlinkService {
    pub async fn email(
        &self,

        headers: &HeaderMap,
        account: auth::EmailAccount,
    ) -> Result<Json<Empty>, V2Error> {
        const FAILED: &str = "Error while trying to unlink email.";
        let uid = bearer(headers, &self.plane)?;
        let queries = self.plane.q().await?;
        if !account.email.is_empty() {
            let address = account.email.to_lowercase();
            let user = queries.get_user_by_id(uid).await.map_err(refused(FAILED))?;
            if user.email.as_deref() != Some(address.as_str()) {
                return Err(last_account());
            }
        }
        queries.unlink_email(uid).await.map_err(refused(FAILED))?;
        Ok(Json(Empty {}))
    }

    pub async fn custom(
        &self,

        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        const FAILED: &str = "Error while trying to unlink custom ID.";
        let uid = bearer(headers, &self.plane)?;
        let queries = self.plane.q().await?;
        if !account.id.is_empty() {
            let user = queries.get_user_by_id(uid).await.map_err(refused(FAILED))?;
            if user.custom_id.as_deref() != Some(account.id.as_str()) {
                return Err(last_account());
            }
        }
        queries
            .unlink_custom_id(uid)
            .await
            .map_err(refused(FAILED))?;
        Ok(Json(Empty {}))
    }
}
