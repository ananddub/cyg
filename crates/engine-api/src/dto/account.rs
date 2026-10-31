use serde::{Deserialize, Serialize};

use crate::dto::console::account::WalletLedgerRow;
use crate::repositories::account::ListDevicePushTokensRow;
use crate::repositories::models::Users;

#[derive(Debug, Default, Deserialize)]
pub struct WalletLedgerMetadataItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdatePlayerWalletRequest {
    #[serde(default)]
    pub wallet_items: serde_json::Value,
    #[serde(default)]
    pub ledger_items: Vec<WalletLedgerMetadataItem>,
}

#[derive(Debug, Serialize)]
pub struct PlayerWalletResponse {
    pub wallet: serde_json::Value,
    pub ledger_items: Vec<WalletLedgerRow>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub lang_tag: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub metadata: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetAccountQuery {
    #[serde(default)]
    pub vars: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePasswordRequest {
    pub current_password: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceAccountResponse {
    pub id: String,
    pub vars: serde_json::Value,
    pub provider: String,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: String,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub metadata: serde_json::Value,
    pub wallet: serde_json::Value,
    pub email: Option<String>,
    pub facebook_id: Option<String>,
    pub google_id: Option<String>,
    pub gamecenter_id: Option<String>,
    pub steam_id: Option<String>,
    pub apple_id: Option<String>,
    pub facebook_instant_game_id: Option<String>,
    pub custom_id: Option<String>,
    pub edge_count: i32,
    pub create_time: String,
    pub update_time: String,
    pub verify_time: String,
    pub disable_time: String,
    pub device_accounts: Vec<DeviceAccountResponse>,
}

impl AccountResponse {
    pub fn from_rows(user: Users, devices: Vec<ListDevicePushTokensRow>) -> Self {
        AccountResponse {
            user_id: user.id.to_string(),
            username: user.username,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
            lang_tag: user.lang_tag,
            location: user.location,
            timezone: user.timezone,
            metadata: user.metadata,
            wallet: user.wallet,
            email: user.email,
            facebook_id: user.facebook_id,
            google_id: user.google_id,
            gamecenter_id: user.gamecenter_id,
            steam_id: user.steam_id,
            apple_id: user.apple_id,
            facebook_instant_game_id: user.facebook_instant_game_id,
            custom_id: user.custom_id,
            edge_count: user.edge_count,
            create_time: user.create_time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            update_time: user.update_time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            verify_time: user.verify_time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            disable_time: user.disable_time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            device_accounts: devices
                .into_iter()
                .map(|d| DeviceAccountResponse {
                    id: d.id,
                    vars: serde_json::json!({
                        "push_token_ios": d.push_token_ios,
                        "push_token_android": d.push_token_android,
                        "push_token_amazon": d.push_token_amazon,
                        "push_token_huawei": d.push_token_huawei,
                        "push_token_web": d.push_token_web,
                    }),
                    provider: d.provider,
                })
                .collect(),
        }
    }
}
