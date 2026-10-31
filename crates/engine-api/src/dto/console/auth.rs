use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
pub struct AuthenticateRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub mfa: String,
}

#[derive(Debug, Serialize)]
pub struct ConsoleSessionResponse {
    pub token: String,
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    pub role: &'static str,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutRequest {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct MfaCodeRequest {
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct MfaEnrolmentResponse {
    pub secret: String,
    pub uri: String,
    pub recovery_codes: Vec<String>,
}
