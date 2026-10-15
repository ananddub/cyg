//! Parties (squads): BaaS `pipeline_party.go` parity.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

use crate::infra::ws::envelope::{Party, Presence};

pub mod admin;
pub mod cleanup;
pub mod data;
pub mod lifecycle;
pub mod matchmaker;

pub use admin::{close, promote, update};
pub use cleanup::cleanup;
pub use data::data_send;
pub use lifecycle::{create, join, leave};
pub use matchmaker::{matchmaker_add, matchmaker_remove};

const MAX_PARTY_SIZE: i32 = 4;

struct PartyState {
    id: Uuid,
    leader_sid: Uuid,
    open: bool,
    max_size: i32,
    members: HashMap<Uuid /*sid*/, Presence>,
}

static PARTIES: OnceLock<RwLock<HashMap<Uuid, PartyState>>> = OnceLock::new();

fn parties() -> &'static RwLock<HashMap<Uuid, PartyState>> {
    PARTIES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Project the registry row to the wire `Party`, excluding the caller from
/// `presences` (that member is carried in `self_presence`).
fn wire(p: &PartyState, self_sid: Uuid) -> Party {
    let me = p.members.get(&self_sid).cloned().unwrap_or_default();
    Party {
        party_id: p.id.simple().to_string(),
        open: p.open,
        max_size: p.max_size,
        self_presence: me,
        leader: p.members.get(&p.leader_sid).cloned().unwrap_or_default(),
        presences: p
            .members
            .values()
            .filter(|x| x.session_id != self_sid.simple().to_string())
            .cloned()
            .collect(),
    }
}

fn wire_of(p: &PartyState, self_sid: Uuid) -> Party {
    wire(p, self_sid)
}
