use super::*;
use std::sync::Arc;

use crate::services::plane::Plane;

pub struct RealtimePresenceService {
    pub plane: Arc<Plane>,
    pub presences: &'static FastMap<Uuid, HashMap<Uuid, (String, String)>>,
}
#[singleton]
impl RealtimePresenceService {
    pub fn new(plane: Arc<Plane>) -> Self {
        static PRESENCES: OnceLock<FastMap<Uuid, HashMap<Uuid, (String, String)>>> =
            OnceLock::new();
        Self {
            plane,
            presences: PRESENCES.get_or_init(new_fast_map),
        }
    }

    pub fn global() -> &'static RealtimePresenceService {
        static INSTANCE: OnceLock<RealtimePresenceService> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            RealtimePresenceService::new(crate::app::state::current_state().plane())
        })
    }

    pub fn get_online_for(uids: &[Uuid]) -> Vec<StatusPresence> {
        Self::global().online_for(uids)
    }

    pub fn get_presence_of(session: &Session) -> Presence {
        Self::global().presence_of(session)
    }

    pub fn handle_disconnect(sid: Uuid) {
        Self::global().on_disconnect(sid);
    }
}
