use crate::services::plane::Plane;

use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use super::bearer;
use crate::dto::Empty;
use crate::dto::auth;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::repositories::link::LinkEmailParams;
use crate::services::auth::validate;
use crate::utils::link::{failure, guarded};

async fn link_device(plane: &Plane, device_id: &str, uid: Uuid) -> Result<(), V2Error> {
    const FAILED: &str = "Error linking Device ID.";
    let queries = plane.q().await?;
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

impl super::LinkService {
    pub async fn device(
        &self,

        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, &self.plane)?;
        validate::id_field(&account.id, "Device ID", 10)?;
        link_device(&self.plane, &account.id, uid).await?;
        Ok(Json(Empty {}))
    }

    pub async fn custom(
        &self,

        headers: &HeaderMap,
        account: auth::IdAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, &self.plane)?;
        custom_id(&account.id)?;
        let queries = self.plane.q().await?;
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

        headers: &HeaderMap,
        account: auth::EmailAccount,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = bearer(headers, &self.plane)?;
        let email = validate::email_fields(&account.email, &account.password)?;
        let queries = self.plane.q().await?;
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
