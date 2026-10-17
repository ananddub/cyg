//! Server settings (`setting`), as the console lists and writes them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::Setting;

/// One setting row. `value` is the raw JSON the server stores.
#[derive(Debug, Serialize)]
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

/// `PUT /v2/console/setting` — a whole setting, created if it is new. Naming the
/// value rather than patching it means a write can never leave a half-updated
/// document behind.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateSettingRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: serde_json::Value,
}
