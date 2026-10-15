//! Server settings (`setting`), as the console lists them. Read-only for now:
//! changing a setting is a Phase-2 mutation.

use chrono::{DateTime, Utc};
use serde::Serialize;

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
