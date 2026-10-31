use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUserDevicesRow {
    pub id: String,
    pub user_id: Uuid,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDevicePushTokensRow {
    pub id: String,
    pub push_token_ios: String,
    pub push_token_android: String,
    pub push_token_amazon: String,
    pub push_token_huawei: String,
    pub push_token_web: String,
    pub provider: String,
}

pub struct UpdateAccountParams {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: Option<String>,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub metadata: serde_json::Value,
    pub username: String,
    pub id: Uuid,
}

pub struct UpdateDevicePushTokensParams {
    pub push_token_ios: String,
    pub roid: String,
    pub push_token_amazon: String,
    pub push_token_huawei: String,
    pub push_token_web: String,
    pub id: String,
    pub user_id: Uuid,
}
