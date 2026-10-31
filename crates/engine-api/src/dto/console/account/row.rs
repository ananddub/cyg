use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{Users, UsersNotes, WalletLedger};

#[derive(Debug, Serialize)]
pub struct AccountRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub lang_tag: String,
    pub location: String,
    pub timezone: String,
    pub email: String,
    pub metadata: serde_json::Value,
    pub wallet: serde_json::Value,
    pub edge_count: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub disable_time: DateTime<Utc>,
    pub custom_id: String,
    pub facebook_id: String,
    pub google_id: String,
    pub gamecenter_id: String,
    pub steam_id: String,
    pub apple_id: String,
    pub facebook_instant_game_id: String,
}

impl From<Users> for AccountRow {
    fn from(u: Users) -> Self {
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name.unwrap_or_default(),
            avatar_url: u.avatar_url.unwrap_or_default(),
            lang_tag: u.lang_tag,
            location: u.location.unwrap_or_default(),
            timezone: u.timezone.unwrap_or_default(),
            email: u.email.unwrap_or_default(),
            metadata: u.metadata,
            wallet: u.wallet,
            edge_count: u.edge_count,
            create_time: u.create_time,
            update_time: u.update_time,
            disable_time: u.disable_time,
            custom_id: u.custom_id.unwrap_or_default(),
            facebook_id: u.facebook_id.unwrap_or_default(),
            google_id: u.google_id.unwrap_or_default(),
            gamecenter_id: u.gamecenter_id.unwrap_or_default(),
            steam_id: u.steam_id.unwrap_or_default(),
            apple_id: u.apple_id.unwrap_or_default(),
            facebook_instant_game_id: u.facebook_instant_game_id.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountNoteRow {
    pub id: Uuid,
    pub note: String,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<UsersNotes> for AccountNoteRow {
    fn from(n: UsersNotes) -> Self {
        Self {
            id: n.id,
            note: n.note,
            create_time: n.create_time,
            update_time: n.update_time,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletLedgerRow {
    pub id: Uuid,
    pub changeset: serde_json::Value,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<WalletLedger> for WalletLedgerRow {
    fn from(w: WalletLedger) -> Self {
        Self {
            id: w.id,
            changeset: w.changeset,
            metadata: w.metadata,
            create_time: w.create_time,
            update_time: w.update_time,
        }
    }
}
