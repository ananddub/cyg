//! Status presence: online presences and the follow graph, in memory only.
//!
//! BaaS parity: `server/tracker.go` + `pipeline_status.go`. Events are scoped
//! to followers; only username resolution touches the database.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use uuid::Uuid;

use crate::infra::ws::envelope::StatusPresence;
use crate::infra::ws::session;

pub mod disconnect;
pub mod follow;
pub mod online;
pub mod status;

pub use disconnect::on_disconnect;
pub use follow::{follow, unfollow};
pub use online::{online_for, presence_of};
pub use status::update_status;

/// Online presences: uid → sid → (username, status).
static PRESENCES: OnceLock<RwLock<HashMap<Uuid, HashMap<Uuid, (String, String)>>>> = OnceLock::new();

pub(super) fn presences() -> &'static RwLock<HashMap<Uuid, HashMap<Uuid, (String, String)>>> {
    PRESENCES.get_or_init(|| RwLock::new(HashMap::new()))
}

pub(super) fn to_status_presence(uid: Uuid, sid: Uuid, username: &str, status: &str) -> StatusPresence {
    StatusPresence {
        user_id: uid.simple().to_string(),
        session_id: sid.simple().to_string(),
        username: username.to_string(),
        status: status.to_string(),
    }
}

/// Every live socket following `target`.
pub(super) fn followers_of(target: Uuid) -> Vec<Arc<session::Session>> {
    let mut seen = Vec::new();
    for uid in session::online_users() {
        for s in session::sessions_of(uid) {
            if s.follows.read().expect("follows lock").contains(&target) {
                seen.push(s);
            }
        }
    }
    seen
}
