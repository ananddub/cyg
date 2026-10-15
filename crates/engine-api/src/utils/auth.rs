//! BaaS reports a failed insert three ways, and which one is a privacy decision:
//! the username clash is a Conflict because the client chose that name, while a
//! provider-id clash is Internal so nobody can enumerate taken identifiers.

use std::collections::HashMap;

use uuid::Uuid;

use crate::repositories::models::Users;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::dto::session::SessionResponse;

/// the entire ban test: a non-zero `disable_time` runs before any
/// credential check, so a banned account never learns whether its password was right.
pub fn existing(user: Users) -> Result<Users, V2Error> {
    if user.disable_time.timestamp() != 0 {
        return Err(V2Error::permission_denied("User account banned."));
    }
    Ok(user)
}

pub fn lookup_failed(e: sqlx::Error) -> V2Error {
    eprintln!("[v2] user lookup failed: {e}");
    V2Error::internal("Error finding user account.")
}

/// A failed insert; only the username clash is explained to the client.
pub fn insert_failed(e: sqlx::Error) -> V2Error {
    if V2Error::is_unique_violation(&e, "users_username_key") {
        return V2Error::already_exists("Username is already in use.");
    }
    eprintln!("[v2] user insert failed: {e}");
    V2Error::internal("Error finding or creating user account.")
}

/// The reply; `vars` go into the token only, never persisted on the account row.
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
