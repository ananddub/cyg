//! `[control.social]`: third-party identity providers behind
//! `/v2/account/authenticate/{provider}`.
//!
//! A provider with no credentials stays closed: Apple and Steam answer "not
//! configured", Google and Facebook cannot validate an audience and answer
//! "could not authenticate". `allow_unverified` is a local-development escape
//! hatch that maps any token to a stable hashed identity — never ship it set.

/// Provider credentials and verification endpoints.
#[derive(Debug, Clone)]
pub struct SocialConfig {
    /// Accepted Google OAuth client IDs (Android, iOS, Web).
    pub google_client_ids: Vec<String>,
    /// Google ID-token introspection endpoint.
    pub google_tokeninfo_url: String,
    /// Apple `aud` claim, i.e. the app's bundle or services id.
    pub apple_client_id: String,
    /// Apple's public JWKS document.
    pub apple_keys_url: String,
    pub facebook_app_id: String,
    pub facebook_app_secret: String,
    /// Graph API base, without a version segment.
    pub facebook_graph_url: String,
    /// Steam app id the ticket must belong to.
    pub steam_app_id: u32,
    pub steam_web_api_key: String,
    /// Steam Web API base.
    pub steam_api_url: String,
    /// Accept any non-empty token and derive the identity from its hash.
    pub allow_unverified: bool,
}

impl Default for SocialConfig {
    fn default() -> Self {
        Self {
            google_client_ids: Vec::new(),
            google_tokeninfo_url: "https://oauth2.googleapis.com/tokeninfo".into(),
            apple_client_id: String::new(),
            apple_keys_url: "https://appleid.apple.com/auth/keys".into(),
            facebook_app_id: String::new(),
            facebook_app_secret: String::new(),
            facebook_graph_url: "https://graph.facebook.com".into(),
            steam_app_id: 0,
            steam_web_api_key: String::new(),
            steam_api_url: "https://api.steampowered.com".into(),
            allow_unverified: false,
        }
    }
}

impl SocialConfig {
    pub fn google_configured(&self) -> bool {
        !self.google_client_ids.is_empty()
    }

    pub fn apple_configured(&self) -> bool {
        !self.apple_client_id.trim().is_empty()
    }

    pub fn facebook_configured(&self) -> bool {
        !self.facebook_app_id.trim().is_empty() && !self.facebook_app_secret.trim().is_empty()
    }

    pub fn steam_configured(&self) -> bool {
        self.steam_app_id > 0 && !self.steam_web_api_key.trim().is_empty()
    }
}

pub(crate) fn split_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}
