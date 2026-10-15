//! The three providers whose credentials are our own database's business: device,
//! custom id and email — look the account up by its identity, verify it, return it or
//! create it. A banned account is refused before any secret is compared, so a disabled
//! user never learns whether its password was right.

use std::collections::HashMap;

use axum::Json;
use uuid::Uuid;

use super::validate::Login;
use crate::ApiState;
use crate::repositories::auth::{CreateUserWithCustomIdParams, CreateUserWithEmailParams};
use crate::services::auth::q;
use crate::utils::auth::{existing, insert_failed, lookup_failed, session};
use crate::error::V2Error;
use crate::infra::jwt;
use crate::dto::session::SessionResponse;

/// Device ids live in `user_device`, keyed by the id itself, so a device row can
/// name an account since linked to a vendor — refused rather than logged into.
pub async fn device(
    state: &ApiState,
    id: &str,
    username: &str,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state)?;
    match queries.get_device_by_id(id).await {
        Ok(row) => {
            if !row.provider.is_empty() {
                return Err(V2Error::permission_denied(
                    "User account uses provider-linked identifier.",
                ));
            }
            // The device row names the account but holds none of its details, so the user is re-read.
            let user = queries
                .get_user_by_id(row.user_id)
                .await
                .map_err(lookup_failed)?;
            let user = existing(user)?;
            Ok(session(user.id, &user.username, false, vars))
        }
        Err(sqlx::Error::RowNotFound) if create => {
            let uid = create_device(state, id, username).await?;
            Ok(session(uid, username, true, vars))
        }
        Err(sqlx::Error::RowNotFound) => Err(V2Error::not_found("User account not found.")),
        Err(e) => Err(lookup_failed(e)),
    }
}

/// Custom ids are a nullable unique column on `users`, not a device row.
pub async fn custom(
    state: &ApiState,
    id: &str,
    username: &str,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state)?;
    match queries.get_user_by_custom_id(id).await {
        Ok(user) => {
            let user = existing(user)?;
            Ok(session(user.id, &user.username, false, vars))
        }
        Err(sqlx::Error::RowNotFound) if create => {
            let user = queries
                .create_user_with_custom_id(&CreateUserWithCustomIdParams {
                    id: Uuid::new_v4(),
                    username: username.to_owned(),
                    custom_id: id.to_owned(),
                })
                .await
                .map_err(insert_failed)?;
            Ok(session(user.id, &user.username, true, vars))
        }
        Err(sqlx::Error::RowNotFound) => Err(V2Error::not_found("User account not found.")),
        Err(e) => Err(lookup_failed(e)),
    }
}

/// Email accounts hold the only secret a client has to prove; a username login is the
/// same endpoint with the address omitted, and never creates.
pub async fn email(
    state: &ApiState,
    login: Login,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state)?;
    match login {
        Login::Email {
            email,
            password,
            username,
        } => match queries.get_user_by_email(&email).await {
            Ok(user) => {
                let user = existing(user)?;
                check_password(&password, &user.password)?;
                Ok(session(user.id, &user.username, false, vars))
            }
            Err(sqlx::Error::RowNotFound) if create => {
                let user = queries
                    .create_user_with_email(&CreateUserWithEmailParams {
                        id: Uuid::new_v4(),
                        username,
                        email,
                        password: jwt::hash_password(&password)?,
                    })
                    .await
                    .map_err(insert_failed)?;
                Ok(session(user.id, &user.username, true, vars))
            }
            Err(sqlx::Error::RowNotFound) => Err(V2Error::not_found("User account not found.")),
            Err(e) => Err(lookup_failed(e)),
        },
        Login::Username { username, password } => {
            let user = match queries.get_user_by_username(&username).await {
                Ok(user) => existing(user)?,
                Err(sqlx::Error::RowNotFound) => {
                    return Err(V2Error::not_found("User account not found."));
                }
                Err(e) => return Err(lookup_failed(e)),
            };
            check_password(&password, &user.password)?;
            Ok(session(user.id, &user.username, false, vars))
        }
    }
}

/// An account made by another provider has no password, reported identically to a
/// wrong one: telling them apart would confirm which addresses have accounts.
fn check_password(password: &str, stored: &Option<Vec<u8>>) -> Result<(), V2Error> {
    if !jwt::verify_password(password, stored.as_deref().unwrap_or_default()) {
        return Err(V2Error::unauthenticated("Invalid credentials."));
    }
    Ok(())
}

/// The device creation BaaS wraps in one transaction: the account is inserted only
/// while the device id is *still* absent, so two concurrent first logins cannot both mint.
async fn create_device(
    state: &ApiState,
    device_id: &str,
    username: &str,
) -> Result<Uuid, V2Error> {
    let db = state.db.as_ref().ok_or_else(V2Error::no_db)?;
    let id = Uuid::new_v4();
    let mut tx = db.begin().await.map_err(insert_failed)?;

    let inserted = sqlx::query(
        "INSERT INTO users (id, username, create_time, update_time) \
         SELECT $1, $2, now(), now() \
         WHERE NOT EXISTS (SELECT id FROM user_device WHERE id = $3::VARCHAR)",
    )
    .bind(id)
    .bind(username)
    .bind(device_id)
    .execute(&mut *tx)
    .await
    .map_err(insert_failed)?;
    if inserted.rows_affected() != 1 {
        return Err(V2Error::internal("Error finding or creating user account."));
    }

    sqlx::query("INSERT INTO user_device (id, user_id) VALUES ($1, $2)")
        .bind(device_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(insert_failed)?;
    tx.commit().await.map_err(insert_failed)?;
    Ok(id)
}
