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

    /// A role named on the wire. `None` for anything unrecognised, so a caller can
    /// answer 400 instead of silently widening or narrowing access.
    pub fn from_name(name: &str) -> Option<ConsoleRole> {
        match name {
            "admin" => Some(ConsoleRole::Admin),
            "developer" => Some(ConsoleRole::Developer),
            "maintainer" => Some(ConsoleRole::Maintainer),
            "readonly" => Some(ConsoleRole::Readonly),
            _ => None,
        }
    }

    /// The `acl` document a role is stored as. The number is authoritative for
    /// [`from_acl`](ConsoleRole::from_acl); the boolean is written alongside it so a
    /// reader that only looks at `admin` agrees.
    pub fn to_acl(self) -> Value {
        serde_json::json!({ "role": self.as_u8(), "admin": self.is_admin() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_round_trips_through_its_names_and_its_acl() {
        for role in [
            ConsoleRole::Admin,
            ConsoleRole::Developer,
            ConsoleRole::Maintainer,
            ConsoleRole::Readonly,
        ] {
            assert_eq!(ConsoleRole::from_name(role.as_str()), Some(role));
            assert_eq!(ConsoleRole::from_acl(&role.to_acl()), role);
            assert_eq!(ConsoleRole::from_u8(role.as_u8()), role);
        }
    }

    #[test]
    fn an_unknown_name_or_acl_never_widens_access() {
        assert_eq!(ConsoleRole::from_name("root"), None);
        assert_eq!(
            ConsoleRole::from_acl(&serde_json::json!({})),
            ConsoleRole::Readonly
        );
        assert_eq!(
            ConsoleRole::from_acl(&serde_json::json!({ "role": 99 })),
            ConsoleRole::Readonly
        );
        // The boolean alone is enough, which is what the fine-grained migration writes.
        assert_eq!(
            ConsoleRole::from_acl(&serde_json::json!({ "admin": true })),
            ConsoleRole::Admin
        );
    }
}
