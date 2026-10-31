use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("users", schemaless)]
#[index(name = "users_username_key", fields = "username", unique)]
#[index(name = "users_email_key", fields = "email", unique)]
#[index(name = "users_facebook_id_key", fields = "facebook_id", unique)]
#[index(name = "users_google_id_key", fields = "google_id", unique)]
#[index(name = "users_gamecenter_id_key", fields = "gamecenter_id", unique)]
#[index(name = "users_steam_id_key", fields = "steam_id", unique)]
#[index(name = "users_custom_id_key", fields = "custom_id", unique)]
#[index(
    name = "users_facebook_instant_game_id_key",
    fields = "facebook_instant_game_id",
    unique
)]
#[index(name = "users_apple_id_key", fields = "apple_id", unique)]
#[index(name = "idx_users_game", fields = "game_id")]
pub struct Users {
    #[field(thing)]
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: String,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub metadata: serde_json::Value,
    pub wallet: serde_json::Value,
    pub email: Option<String>,
    #[serde(with = "bytes::b64_opt")]
    #[field(ty = "string")]
    pub password: Option<Vec<u8>>,
    pub facebook_id: Option<String>,
    pub google_id: Option<String>,
    pub gamecenter_id: Option<String>,
    pub steam_id: Option<String>,
    pub custom_id: Option<String>,
    pub edge_count: i32,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub verify_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub disable_time: DateTime<Utc>,
    pub facebook_instant_game_id: Option<String>,
    pub apple_id: Option<String>,
    pub game_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("users_notes", schemaless)]
#[index(name = "users_notes_user_id", fields = "user_id")]
pub struct UsersNotes {
    #[field(thing)]
    pub id: Uuid,
    pub user_id: Uuid,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub note: String,
    pub create_id: Option<Uuid>,
    pub update_id: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("wallet_ledger", schemaless)]
#[index(name = "wallet_ledger_user_id", fields = "user_id")]
pub struct WalletLedger {
    #[field(thing)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub changeset: serde_json::Value,
    pub metadata: serde_json::Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    pub game_id: Option<Uuid>,
}
