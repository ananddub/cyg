use std::collections::HashMap;

use uuid::Uuid;

use crate::db::DbError;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::repositories::models::Users;

pub fn existing(user: Users) -> Result<Users, V2Error> {
    if user.disable_time.timestamp() != 0 {
        return Err(V2Error::permission_denied("User account banned."));
    }
    Ok(user)
}

pub fn lookup_failed(e: DbError) -> V2Error {
    tracing::error!("[v2] user lookup failed: {e}");
    V2Error::internal("Error finding user account.")
}

pub fn insert_failed(e: DbError) -> V2Error {
    if V2Error::is_unique_violation(&e, "users_username_key") {
        return V2Error::already_exists("Username is already in use.");
    }
    tracing::error!("[v2] user insert failed: {e}");
    V2Error::internal("Error finding or creating user account.")
}

pub fn session(
    uid: Uuid,
    username: &str,
    created: bool,
    vars: HashMap<String, String>,
) -> axum::Json<SessionResponse> {
    let (token, refresh_token) = jwt::mint_token_pair(uid, username, vars);
    axum::Json(SessionResponse {
        created,
        token,
        refresh_token,
    })
}
