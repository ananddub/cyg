//! The session shapes: the token pair every `Authenticate*` call returns, plus the
//! refresh and logout bodies.
//!
//! [`SessionResponse`] lives here because it is what the client receives — `jwt`
//! mints it, but the wire contract is this layer's business.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The reply every `Authenticate*` and `SessionRefresh` call returns. The gateway
/// marshal omits `created` when it is false, since it drops default values.
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub created: bool,
    pub token: String,
    pub refresh_token: String,
}

/// `api.SessionRefreshRequest`. Absent `vars` means "keep the ones in the token",
/// while an empty map is a deliberate clear, so the two stay distinguishable.
#[derive(Debug, Default, Deserialize)]
pub struct RefreshRequest {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub vars: Option<HashMap<String, String>>,
}

/// `api.SessionLogoutRequest`. Either half may be omitted.
#[derive(Debug, Default, Deserialize)]
pub struct LogoutRequest {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub refresh_token: String,
}
