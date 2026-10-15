//! Console operators (`console_user`) as the console lists them.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::infra::console::ConsoleRole;
use crate::repositories::console::ConsoleUserRow as Row;

/// A console operator. The password and MFA secrets are absent by construction —
/// the query never selects them.
#[derive(Debug, Serialize)]
pub struct ConsoleUserView {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    /// The role name derived from the ACL document, for display.
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
