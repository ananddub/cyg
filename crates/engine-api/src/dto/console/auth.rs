use serde::{Deserialize, Serialize};

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct AuthenticateRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub mfa: String,
}

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct ConsoleSessionResponse {
    pub token: String,
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    pub role: String,
}

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
#[oai(rename = "ConsoleLogoutRequest")]
pub struct LogoutRequest {
    #[serde(default)]
    pub token: String,
}

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct MfaCodeRequest {
    #[serde(default)]
    pub code: String,
}

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct MfaEnrolmentResponse {
    pub secret: String,
    pub uri: String,
    pub recovery_codes: Vec<String>,
}

/// Whether this process runs in local development mode.
#[derive(Debug, Clone, serde::Serialize, poem_openapi::Object)]
pub struct ConsoleModeResponse {
    pub dev: bool,
}
