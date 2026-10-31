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
use crate::services::auth::validate;
use crate::services::deps::q;
use crate::utils::link::{failure, guarded};

async fn link_device(state: &ApiState, device_id: &str, uid: Uuid) -> Result<(), V2Error> {
    const FAILED: &str = "Error linking Device ID.";
    let queries = q(state).await?;
    let owned = queries
        .count_owned_device(device_id, uid)
        .await
        .map_err(|e| failure(FAILED, e))?;
    if owned == 0 {
        if let Err(e) = queries.insert_device(device_id, uid).await {
            if V2Error::is_any_unique_violation(&e) {
                return Err(V2Error::already_exists("Device ID already in use."));
            }
            return Err(failure(FAILED, e));
        }
    }
    queries
        .touch_user(uid)
        .await
        .map_err(|e| failure(FAILED, e))?;
    Ok(())
}

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

    pub async fn custom(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        custom_id(&account.id)?;
        let queries = q(state).await?;
        queries
            .link_custom_id(&account.id, uid)
            .await
            .map_err(guarded(
                "Custom ID is already in use.",
                "Error while trying to link Custom ID.",
            ))?;
        Ok(Json(Empty {}))
    }

    pub async fn email(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        account: auth::EmailAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, state)?;
        let email = email_fields(&account.email, &account.password)?;
        let queries = q(state).await?;
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
