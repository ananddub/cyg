//! Console operators (`console_user`) as the console lists them, and the bodies
//! that create and edit one.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::infra::console::ConsoleRole;
use crate::repositories::console::FindConsoleUserRow as Row;

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

/// `POST /v2/console/user` — a new operator. The role is named rather than sent as
/// an ACL document: the console's own roles are the only ones an operator may
/// assign, and a name cannot smuggle in a permission nobody reviewed.
#[derive(Debug, Default, Deserialize)]
pub struct CreateConsoleUserRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    /// `admin`, `developer`, `maintainer` or `readonly`.
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub mfa_required: bool,
}

/// `PUT /v2/console/user/{id}` — the mutable profile. The password is absent by
/// design: it has its own route, so editing a profile cannot change how someone
/// signs in.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateConsoleUserRequest {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub email: String,
    /// `admin`, `developer`, `maintainer` or `readonly`.
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub mfa_required: bool,
}

/// `POST /v2/console/user/{id}/password` — an operator resetting someone else's
/// credential, which is why it is its own route and carries nothing else.
#[derive(Debug, Default, Deserialize)]
pub struct SetConsolePasswordRequest {
    #[serde(default)]
    pub password: String,
}
