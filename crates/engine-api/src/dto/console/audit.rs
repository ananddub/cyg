//! The audit log: what a console operator did, and the ACL templates an operator
//! role is defined from.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::repositories::models::{ConsoleAclTemplate, ConsoleAuditLog};

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
