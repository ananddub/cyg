use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::Setting;

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct SettingRow {
    pub name: String,
    pub value: serde_json::Value,
    pub update_time: DateTime<Utc>,
}

impl From<Setting> for SettingRow {
    fn from(s: Setting) -> Self {
        Self {
            name: s.name,
            value: s.value,
            update_time: s.update_time,
        }
    }
}

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct UpdateSettingRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: serde_json::Value,
}
