use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub created: bool,
    pub token: String,
    pub refresh_token: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct RefreshRequest {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutRequest {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub refresh_token: String,
}
