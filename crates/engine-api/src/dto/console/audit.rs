//! The audit log: what a console operator did, and the ACL templates an operator
//! role is defined from.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{ConsoleAclTemplate, ConsoleAuditLog};

/// `GET /v2/console/audit/log` — the trail with the filters the console offers.
/// Every text field is empty for "any", which is also how the SQL reads it.
#[derive(Debug, Default, Deserialize)]
pub struct AuditLogQuery {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub resource: String,
    #[serde(default)]
    pub username: String,
    /// Inclusive RFC 3339 bounds on the entry's `create_time`.
    #[serde(default)]
    pub after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub before: Option<DateTime<Utc>>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

impl AuditLogQuery {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(25).clamp(1, 100)
    }

    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

/// `POST` and `PUT /v2/console/acl/template` — a named permission document an
/// operator can be assigned from.
#[derive(Debug, Default, Deserialize)]
pub struct AclTemplateRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub acl: serde_json::Value,
}

/// One audit entry.
#[derive(Debug, Serialize)]
pub struct AuditLogRow {
    pub create_time: DateTime<Utc>,
    pub id: Uuid,
    pub console_user_id: Uuid,
    pub console_username: String,
    pub email: String,
    pub action: String,
    pub resource: String,
    pub message: String,
    pub metadata: serde_json::Value,
}

impl From<ConsoleAuditLog> for AuditLogRow {
    fn from(a: ConsoleAuditLog) -> Self {
        Self {
            create_time: a.create_time,
            id: a.id,
            console_user_id: a.console_user_id,
            console_username: a.console_username,
            email: a.email,
            action: a.action,
            resource: a.resource,
            message: a.message,
            metadata: a.metadata,
        }
    }
}

/// One ACL template: a named, reusable permission document.
#[derive(Debug, Serialize)]
pub struct AclTemplateRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub acl: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<ConsoleAclTemplate> for AclTemplateRow {
    fn from(t: ConsoleAclTemplate) -> Self {
        Self {
            id: t.id,
            name: t.name,
            description: t.description,
            acl: t.acl,
            create_time: t.create_time,
            update_time: t.update_time,
        }
    }
}

/// `GET /v2/console/audit/log/users` — the operators who appear in the trail, one
/// entry each, most active first. Mirrors the console's users list, which the audit
/// view reads to fill its username filter.
#[derive(Debug, Serialize)]
pub struct AuditLogUsers {
    pub usernames: Vec<String>,
}
