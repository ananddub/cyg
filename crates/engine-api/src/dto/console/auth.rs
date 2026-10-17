//! `/v2/console/authenticate` — the console login and logout bodies.

use serde::{Deserialize, Serialize};

/// The console login body. `mfa` is the authenticator code, required only when the
/// operator has MFA enrolled — a login without one is refused once a secret exists.
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

/// `POST /v2/console/mfa/verify` and the body that disables MFA: the digit code an
/// authenticator is showing, or one of the recovery codes written down at
/// enrolment.
#[derive(Debug, Default, Deserialize)]
pub struct MfaCodeRequest {
    #[serde(default)]
    pub code: String,
}

/// The enrolment reply. `secret` is what a client types in by hand and `uri` is
/// what it scans; `recovery_codes` are shown exactly once, because only their
/// hashes are stored.
#[derive(Debug, Serialize)]
pub struct MfaEnrolmentResponse {
    pub secret: String,
    pub uri: String,
    pub recovery_codes: Vec<String>,
}
