//! `PUT /v2/account` and the password change that goes with it.
//!
//! Profile fields are one `COALESCE` statement, so an omitted field keeps its value;
//! the custom id is separate because its unique index can collide with another account.

use axum::Json;

use crate::dto::account::{
    AccountResponse, GetAccountQuery, UpdateAccountRequest, UpdatePasswordRequest,
};
use crate::dto::Empty;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::app::middleware::Authed;
use crate::ApiState;

impl super::AccountService {
    pub async fn update_account(
        &self,
        state: &ApiState,
        auth: &Authed,
        _params: GetAccountQuery,
        req: UpdateAccountRequest,
    ) -> Result<Json<AccountResponse>, V2Error> {
        let db = state.db.as_ref().ok_or_else(V2Error::no_db)?;
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let uid = auth.uid();
        let existing = queries.get_user_by_id(uid).await?;

        let metadata = match req.metadata.as_deref() {
            None => existing.metadata.clone(),
            Some("") => serde_json::json!({}),
            Some(raw) => serde_json::from_str(raw).map_err(|_| V2Error::invalid_argument("invalid account metadata"))?,
        };
        sqlx::query(
            "UPDATE users SET display_name = COALESCE($2, display_name), avatar_url = COALESCE($3, avatar_url), \
             lang_tag = COALESCE($4, lang_tag), location = COALESCE($5, location), timezone = COALESCE($6, timezone), \
             metadata = $7, username = COALESCE($8, username), update_time = now() WHERE id = $1",
        )
        .bind(uid)
        .bind(req.display_name)
        .bind(req.avatar_url)
        .bind(req.lang_tag)
        .bind(req.location)
        .bind(req.timezone)
        .bind(metadata)
        .bind(req.username)
        .execute(db)
        .await?;

        // A custom id may move between accounts only via unlink first, matching
        // BaaS: the column has a unique index; a collision is already-exists.
        if let Some(cid) = req.custom_id.filter(|s| !s.is_empty()) {
            sqlx::query("UPDATE users SET custom_id = $2 WHERE id = $1")
                .bind(uid)
                .bind(cid)
                .execute(db)
                .await
                .map_err(|e| match &e {
                    sqlx::Error::Database(d) if d.message().contains("duplicate key") => {
                        V2Error::already_exists("custom id already in use by another account")
                    }
                    _ => V2Error::from_sqlx(e),
                })?;
        }

        let user = queries.get_user_by_id(uid).await?;
        let devices = queries.list_device_push_tokens(uid).await.unwrap_or_default();
        Ok(Json(AccountResponse::from_rows(user, devices)))
    }

    pub async fn update_password(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: UpdatePasswordRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = state.q.clone().ok_or_else(V2Error::no_db)?;
        let user = queries.get_user_by_id(auth.uid()).await?;
        let stored = user.password.clone().unwrap_or_default();
        if stored.is_empty() || !jwt::verify_password(&req.current_password, &stored) {
            return Err(V2Error::unauthenticated("invalid credentials"));
        }
        if req.password.len() < 8 {
            return Err(V2Error::invalid_argument("password must be at least 8 characters long"));
        }
        queries.update_password(&jwt::hash_password(&req.password)?, user.id).await?;
        Ok(Json(Empty {}))
    }
}
