use super::*;

pub(crate) fn new_device(id: &str, user_id: Uuid, provider: &str) -> UserDevice {
    UserDevice {
        id: id.to_string(),
        user_id,
        preferences: serde_json::json!({}),
        push_token_amazon: String::new(),
        push_token_android: String::new(),
        push_token_huawei: String::new(),
        push_token_ios: String::new(),
        push_token_web: String::new(),
        provider: provider.to_string(),
    }
}
