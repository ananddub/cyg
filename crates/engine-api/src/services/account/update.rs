use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::dto::account::{
    AccountResponse, GetAccountQuery, UpdateAccountRequest, UpdatePasswordRequest,
};
use crate::error::V2Error;
use crate::infra::jwt;
use crate::repositories::account::UpdateAccountParams;

impl super::AccountService {
    pub async fn update_account(
        &self,
        state: &ApiState,
        auth: &Authed,
        _params: GetAccountQuery,
        req: UpdateAccountRequest,
    ) -> Result<Json<AccountResponse>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        let uid = auth.uid();
        let existing = queries.get_user_by_id(uid).await?;

        let metadata = match req.metadata.as_deref() {
            None => existing.metadata.clone(),
            Some("") => serde_json::json!({}),
            Some(raw) => serde_json::from_str(raw)
                .map_err(|_| V2Error::invalid_argument("invalid account metadata"))?,
        };
        let display_name = req.display_name.or(existing.display_name);
        let avatar_url = req.avatar_url.or(existing.avatar_url);
        let lang_tag = req.lang_tag.or(Some(existing.lang_tag));
        let location = req.location.or(existing.location);
        let timezone = req.timezone.or(existing.timezone);
        let username = req.username.unwrap_or(existing.username);
        queries
            .update_account(&UpdateAccountParams {
                display_name,
                avatar_url,
                lang_tag,
                location,
                timezone,
                metadata,
                username,
                id: uid,
            })
            .await?;

        if let Some(cid) = req.custom_id.filter(|s| !s.is_empty()) {
            queries
                .set_custom_id(uid, &cid)
                .await
                .map_err(|e| match e {
                    DbError::Unique(_) => {
                        V2Error::already_exists("custom id already in use by another account")
                    }
                    other => V2Error::from(other),
                })?;
        }

        let user = queries.get_user_by_id(uid).await?;
        let devices = queries
            .list_device_push_tokens(uid)
            .await
            .unwrap_or_default();
        Ok(Json(AccountResponse::from_rows(user, devices)))
    }

    pub async fn update_password(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: UpdatePasswordRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        let stored = user.password.clone().unwrap_or_default();
        if stored.is_empty() || !jwt::verify_password(&req.current_password, &stored) {
            return Err(V2Error::unauthenticated("invalid credentials"));
        }
        if req.password.len() < 8 {
            return Err(V2Error::invalid_argument(
                "password must be at least 8 characters long",
            ));
        }
        queries
            .update_password(&jwt::hash_password(&req.password)?, user.id)
            .await?;
        Ok(Json(Empty {}))
    }
}
