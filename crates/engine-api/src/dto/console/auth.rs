//! `/v2/console/authenticate` — the console login and logout bodies.

use serde::{Deserialize, Serialize};

/// The console login body. `mfa` is accepted so a client that sends it is not
/// refused for the wrong reason, but a console user with MFA configured is turned
/// away for now rather than let through unverified.
#[derive(Debug, Default, Deserialize)]
pub struct AuthenticateRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub mfa: String,
}

/// The console token plus the identity it carries, so the shell can render its
/// header without decoding the JWT.
#[derive(Debug, Serialize)]
pub struct ConsoleSessionResponse {
    pub token: String,
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// The console role name: `admin`, `developer`, `maintainer` or `readonly`.
    pub role: &'static str,
}

/// `POST /v2/console/authenticate/logout`. The header token is the usual one; the
/// body token lets a caller that has already dropped its header still revoke.
#[derive(Debug, Default, Deserialize)]
pub struct LogoutRequest {
    #[serde(default)]
    pub token: String,
}
