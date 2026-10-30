use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::Users;

use super::row::{AccountNoteRow, WalletLedgerRow};

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountExport {
    pub account: ExportedAccount,
    #[serde(default)]
    pub devices: Vec<ExportedDevice>,
    #[serde(default)]
    pub notes: Vec<AccountNoteRow>,
    #[serde(default)]
    pub wallet_ledger: Vec<WalletLedgerRow>,
    #[serde(default)]
    pub storage: Vec<ExportedStorage>,
    #[serde(default)]
    pub edges: Vec<ExportedEdge>,
    #[serde(default)]
    pub purchases: Vec<serde_json::Value>,
    #[serde(default)]
    pub subscriptions: Vec<serde_json::Value>,
    #[serde(default)]
    pub inventory: Vec<serde_json::Value>,
    #[serde(default)]
    pub leaderboard_records: Vec<serde_json::Value>,
    #[serde(default)]
    pub group_memberships: Vec<serde_json::Value>,
    #[serde(default)]
    pub notifications: Vec<serde_json::Value>,
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
}

impl Default for AccountExport {
    fn default() -> Self {
        Self {
            account: ExportedAccount::default(),
            devices: Vec::new(),
            notes: Vec::new(),
            wallet_ledger: Vec::new(),
            storage: Vec::new(),
            edges: Vec::new(),
            purchases: Vec::new(),
            subscriptions: Vec::new(),
            inventory: Vec::new(),
            leaderboard_records: Vec::new(),
            group_memberships: Vec::new(),
            notifications: Vec::new(),
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportedAccount {
    pub id: Uuid,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub lang_tag: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub wallet: serde_json::Value,
    #[serde(default)]
    pub custom_id: Option<String>,
    #[serde(default)]
    pub facebook_id: Option<String>,
    #[serde(default)]
    pub google_id: Option<String>,
    #[serde(default)]
    pub gamecenter_id: Option<String>,
    #[serde(default)]
    pub steam_id: Option<String>,
    #[serde(default)]
    pub apple_id: Option<String>,
    #[serde(default)]
    pub facebook_instant_game_id: Option<String>,
    #[serde(default)]
    pub edge_count: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub disable_time: DateTime<Utc>,
    pub verify_time: DateTime<Utc>,
}

impl From<Users> for ExportedAccount {
    fn from(u: Users) -> Self {
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            avatar_url: u.avatar_url,
            lang_tag: u.lang_tag,
            location: u.location,
            timezone: u.timezone,
            email: u.email,
            metadata: u.metadata,
            wallet: u.wallet,
            custom_id: u.custom_id,
            facebook_id: u.facebook_id,
            google_id: u.google_id,
            gamecenter_id: u.gamecenter_id,
            steam_id: u.steam_id,
            apple_id: u.apple_id,
            facebook_instant_game_id: u.facebook_instant_game_id,
            edge_count: u.edge_count,
            create_time: u.create_time,
            update_time: u.update_time,
            disable_time: u.disable_time,
            verify_time: u.verify_time,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportedDevice {
    pub id: String,
    #[serde(default)]
    pub provider: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportedStorage {
    pub collection: String,
    pub key: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub version: String,
    pub read: i16,
    pub write: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportedEdge {
    pub destination_id: Uuid,
    pub state: i16,
    pub position: i64,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
