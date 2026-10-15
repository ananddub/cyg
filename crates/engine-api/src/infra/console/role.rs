//! Console roles, and the read/write split the handlers gate on.

use serde_json::Value;

/// The four levels the schema comment names. Least privilege is [`ConsoleRole::Readonly`],
/// so an unrecognised value never widens access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleRole {
    Admin,
    Developer,
    Maintainer,
    Readonly,
}

impl ConsoleRole {
    pub fn as_u8(self) -> u8 {
        match self {
            ConsoleRole::Admin => 1,
            ConsoleRole::Developer => 2,
            ConsoleRole::Maintainer => 3,
            ConsoleRole::Readonly => 4,
        }
    }

    pub fn from_u8(value: u8) -> ConsoleRole {
        match value {
            1 => ConsoleRole::Admin,
            2 => ConsoleRole::Developer,
            3 => ConsoleRole::Maintainer,
            _ => ConsoleRole::Readonly,
        }
    }

    /// Derive a role from a `console_user.acl` document: an explicit `role` wins,
    /// then the `admin` boolean the fine-grained ACL migration writes, then readonly.
    pub fn from_acl(acl: &Value) -> ConsoleRole {
        if let Some(role) = acl.get("role").and_then(Value::as_u64) {
            return ConsoleRole::from_u8(role as u8);
        }
        if acl.get("admin").and_then(Value::as_bool) == Some(true) {
            ConsoleRole::Admin
        } else {
            ConsoleRole::Readonly
        }
    }

    /// Reading is open to every console user; writing needs more than readonly.
    pub fn can_write(self) -> bool {
        !matches!(self, ConsoleRole::Readonly)
    }

    pub fn is_admin(self) -> bool {
        matches!(self, ConsoleRole::Admin)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ConsoleRole::Admin => "admin",
            ConsoleRole::Developer => "developer",
            ConsoleRole::Maintainer => "maintainer",
            ConsoleRole::Readonly => "readonly",
        }
    }
}
