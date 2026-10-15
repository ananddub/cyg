//! The console token's claim set.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::role::ConsoleRole;

/// The claims a console token carries. The field order is the wire order; `id` and
/// `email` are omitted when empty (the config bootstrap superadmin has neither).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleClaims {
    /// The `console_user.id`, or empty for the bootstrap superadmin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Username, shown in the console header.
    pub username: String,
    /// Email, empty for the bootstrap superadmin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// The role as the integer [`ConsoleRole`] maps to.
    pub role: u8,
    /// A per-login token id, so two logins in the same second are still distinct
    /// and a logout revokes exactly one token.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub jti: String,
    /// Expiry, seconds since the epoch. Enforced on decode.
    pub exp: i64,
}

impl ConsoleClaims {
    /// The role. An out-of-range integer reads as least privilege, never wider.
    pub fn role(&self) -> ConsoleRole {
        ConsoleRole::from_u8(self.role)
    }

    /// The console user id, or `None` for the bootstrap superadmin, which has no row.
    pub fn uuid(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.id).ok()
    }
}
