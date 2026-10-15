//! Wire shapes of the `Account*` messages.
//!
//! One set serves auth, linking and unlinking: the `LinkDevice`, `UnlinkDevice` and
//! `AuthenticateDeviceRequest.account` are all `api.AccountDevice`, decoded with `DiscardUnknown`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::app::params::BoolFlag;
use crate::error::V2Error;

/// Device and custom ids: just an id plus the vars echoed into the token.
#[derive(Debug, Default, Deserialize)]
pub struct IdAccount {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

/// Apple, Facebook, Google and Steam: a token the vendor issued.
#[derive(Debug, Default, Deserialize)]
pub struct TokenAccount {
    #[serde(default)]
    pub token: String,
    /// Read only for its type check: a `vars` member that is not an object of
    /// strings is a bad request at the gateway.
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmailAccount {
    #[serde(default)]
    pub email: String,
    /// Ignored by the unlink routes, as the request comment says.
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct GameCenterAccount {
    #[serde(default, alias = "playerId")]
    pub player_id: String,
    #[serde(default, alias = "bundleId")]
    pub bundle_id: String,
    #[serde(default, alias = "timestampSeconds")]
    pub timestamp_seconds: i64,
    #[serde(default)]
    pub salt: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default, alias = "publicKeyUrl")]
    pub public_key_url: String,
    /// See [`TokenAccount::vars`] — type-checked on the wire, never read back.
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct InstantGameAccount {
    #[serde(default, alias = "signedPlayerInfo")]
    pub signed_player_info: String,
    /// See [`TokenAccount::vars`] — type-checked on the wire, never read back.
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

/// The unified `/v2/account/authenticate` and `/v2/account/link` bodies: a named
/// runtime provider plus the opaque payload handed to its function.
#[derive(Debug, Default, Deserialize)]
pub struct ProviderAccount {
    #[serde(default)]
    pub provider: String,
    /// The routes that would pass this to a provider function all answer before
    /// they reach it, so the two members exist to reject a wrongly-typed body.
    #[allow(dead_code)]
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// See [`TokenAccount::vars`] — type-checked on the wire, never read back.
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

/// `LinkSteamRequest` — the one link whose body wraps the account, because the same
/// message carries a `sync` flag (Facebook's arrives as a query parameter instead).
#[derive(Debug, Default, Deserialize)]
pub struct SteamLink {
    #[serde(default)]
    pub account: Option<TokenAccount>,
}

/// The `?username=&create=` half of every `Authenticate*` request. Both are
/// query parameters in the gateway's allowlist, never body fields.
#[derive(Debug, Default, Deserialize)]
pub struct AuthQuery {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub create: Option<String>,
}

impl AuthQuery {
    pub fn create(&self) -> Result<bool, V2Error> {
        Ok(match &self.create {
            Some(raw) => BoolFlag::parse(raw)?,
            None => BoolFlag::Unset,
        }
        .create())
    }
}
