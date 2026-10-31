use std::collections::HashMap;

use serde::Deserialize;

use crate::app::params::BoolFlag;
use crate::error::V2Error;

#[derive(Debug, Default, Deserialize)]
pub struct IdAccount {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TokenAccount {
    #[serde(default)]
    pub token: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmailAccount {
    #[serde(default)]
    pub email: String,
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
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct InstantGameAccount {
    #[serde(default, alias = "signedPlayerInfo")]
    pub signed_player_info: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProviderAccount {
    #[serde(default)]
    pub provider: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    #[allow(dead_code)]
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SteamLink {
    #[serde(default)]
    pub account: Option<TokenAccount>,
}

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
