use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::GameConfigVersion;

#[derive(Debug, Serialize)]
pub struct ConfigVersionRow {
    pub game_id: Uuid,
    pub version: i64,
    pub sha256: String,
    pub is_active: bool,
    pub author: Option<String>,
    pub create_time: DateTime<Utc>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

impl GameConfigVersion {
    /// The list view keeps the bodies out of the payload; the single-version
    /// read and the diff view fill them in.
    pub fn to_list_row(&self) -> ConfigVersionRow {
        ConfigVersionRow {
            game_id: self.game_id,
            version: self.version,
            sha256: self.sha256.clone(),
            is_active: self.is_active,
            author: self.author.clone(),
            create_time: self.create_time,
            body: None,
        }
    }

    pub fn to_full_row(&self) -> ConfigVersionRow {
        ConfigVersionRow {
            body: Some(self.body.clone()),
            ..self.to_list_row()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConfigDiff {
    pub from_version: i64,
    pub to_version: i64,
    pub from_body: serde_json::Value,
    pub to_body: serde_json::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct ConfigHistoryQuery {
    #[serde(default)]
    pub game_id: Option<Uuid>,
}
