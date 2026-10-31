use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::infra::console::ConsoleRole;
use crate::repositories::console::FindConsoleUserRow as Row;

#[derive(Debug, Serialize)]
pub struct ConsoleUserView {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: &'static str,
    pub acl: serde_json::Value,
    pub metadata: serde_json::Value,
    pub mfa_required: bool,
    pub disabled: bool,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<Row> for ConsoleUserView {
    fn from(r: Row) -> Self {
        let role = ConsoleRole::from_acl(&r.acl);
        Self {
            id: r.id,
            username: r.username,
            email: r.email,
            role: role.as_str(),
            acl: r.acl,
            metadata: r.metadata,
            mfa_required: r.mfa_required.unwrap_or(false),
            disabled: r.disable_time.timestamp() != 0,
            create_time: r.create_time,
            update_time: r.update_time,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateConsoleUserRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub mfa_required: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateConsoleUserRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub mfa_required: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct SetConsolePasswordRequest {
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangeConsolePasswordRequest {
    #[serde(default)]
    pub current_password: String,
    #[serde(default)]
    pub password: String,
}
