//! The three links that are pure database writes: device, custom id and email —
//! attach an identifier this server can verify by its own rules, in a guarded statement
//! that refuses to move an identity between accounts. The other seven need a vendor's
//! answer and live in [`super::social`].

use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use super::bearer;
use crate::ApiState;
use crate::dto::Empty;
use crate::dto::auth;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::repositories::link::LinkEmailParams;
use crate::services::auth::{q, validate};
use crate::utils::link::{failure, guarded};

/// `LinkDevice` is a transaction rather than a statement: the id may already belong to
/// *this* account (a no-op that still bumps `update_time`), or clash with another (conflict).
async fn link_device(state: &ApiState, device_id: &str, uid: Uuid) -> Result<(), V2Error> {
    const FAILED: &str = "Error linking Device ID.";
    let db = state.db.as_ref().ok_or_else(V2Error::no_db)?;
    let mut tx = db.begin().await.map_err(|e| failure(FAILED, e))?;

    let owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(id) FROM user_device WHERE id = $1 AND user_id = $2 LIMIT 1",
    )
    .bind(device_id)
    .bind(uid)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| failure(FAILED, e))?;
    if owned == 0 {
        if let Err(e) = sqlx::query("INSERT INTO user_device (id, user_id) VALUES ($1, $2)")
            .bind(device_id)
            .bind(uid)
            .execute(&mut *tx)
            .await
        {
            if V2Error::is_any_unique_violation(&e) {
                return Err(V2Error::already_exists("Device ID already in use."));
            }
            return Err(failure(FAILED, e));
        }
    }
    sqlx::query("UPDATE users SET update_time = now() WHERE id = $1")
        .bind(uid)
        .execute(&mut *tx)
        .await
        .map_err(|e| failure(FAILED, e))?;
    tx.commit().await.map_err(|e| failure(FAILED, e))?;
    Ok(())
}

/// `LinkCustom`'s checks: the same rules as the authentication route with different
/// wording, since the link messages lead with "Invalid …".
fn custom_id(raw: &str) -> Result<(), V2Error> {
    if raw.is_empty() {
        return Err(V2Error::invalid_argument("Custom ID is required."));
    }
    if validate::has_space_or_control(raw) {
        return Err(V2Error::invalid_argument(
            "Invalid custom ID, no spaces or control characters allowed.",
        ));
    }
    if raw.len() < 6 || raw.len() > 128 {
        return Err(V2Error::invalid_argument(
            "Invalid custom ID, must be 6-128 bytes.",
        ));
    }
    Ok(())
}

/// `LinkEmail`'s checks in its order rather than the authentication route's: password
/// length before address format, and an empty address is refused outright.
fn email_fields(email: &str, password: &str) -> Result<String, V2Error> {
    if email.is_empty() || password.is_empty() {
        return Err(V2Error::invalid_argument(
            "Email address and password is required.",
        ));
    }
    if validate::has_space_or_control(email) {
        return Err(V2Error::invalid_argument(
            "Invalid email address, no spaces or control characters allowed.",
        ));
    }
    if password.len() < 8 {
        return Err(V2Error::invalid_argument(
            "Password must be at least 8 characters long.",
        ));
    }
    if !validate::has_email_shape(email) {
        return Err(V2Error::invalid_argument("Invalid email address format."));
    }
    if email.len() < 10 || email.len() > 255 {
        return Err(V2Error::invalid_argument(
            "Invalid email address, must be 10-255 bytes.",
        ));
    }
    Ok(email.to_lowercase())
}

impl super::LinkService {
    /// `POST /v2/account/link/device`.
    pub async fn device(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        validate::id_field(&account.id, "Device ID", 10)?;
        link_device(state, &account.id, uid).await?;
        Ok(Json(Empty {}))
    }

    /// `POST /v2/account/link/custom`.
    pub async fn custom(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        custom_id(&account.id)?;
        let queries = q(state)?;
        queries
            .link_custom_id(&account.id, uid)
            .await
            .map_err(guarded(
                "Custom ID is already in use.",
                "Error while trying to link Custom ID.",
            ))?;
        Ok(Json(Empty {}))
    }

    /// `POST /v2/account/link/email`.
    pub async fn email(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::EmailAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        let email = email_fields(&account.email, &account.password)?;
        let queries = q(state)?;
        // `LinkEmailParams.d` is the password column, which FlashORM reads as bytea;
        // the Argon2 PHC string is the value that belongs in it.
        queries
            .link_email(&LinkEmailParams {
                email,
                d: jwt::hash_password(&account.password)?,
                id: uid,
            })
            .await
            .map_err(guarded(
                "Email is already in use.",
                "Error while trying to link email.",
            ))?;
        Ok(Json(Empty {}))
    }
}
