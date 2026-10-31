use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::role::ConsoleRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleClaims {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    pub role: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub jti: String,
    pub exp: i64,
}

impl ConsoleClaims {
    pub fn role(&self) -> ConsoleRole {
        ConsoleRole::from_u8(self.role)
    }

    pub fn uuid(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.id).ok()
    }
}
