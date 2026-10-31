use serde::{Deserialize, Serialize};

use super::presence::Presence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub party_id: String,
    pub open: bool,
    pub max_size: i32,
    pub self_presence: Presence,
    #[serde(default)]
    pub leader: Presence,
    #[serde(default)]
    pub presences: Vec<Presence>,
}
