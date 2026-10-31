use super::*;

pub struct RealtimePresenceService {
    pub presences: &'static FastMap<Uuid, HashMap<Uuid, (String, String)>>,
}
#[singleton]
impl RealtimePresenceService {
    pub fn new() -> Self {
        static PRESENCES: OnceLock<FastMap<Uuid, HashMap<Uuid, (String, String)>>> =
            OnceLock::new();
        Self {
            presences: PRESENCES.get_or_init(new_fast_map),
        }
    }

    pub fn global() -> &'static RealtimePresenceService {
        static INSTANCE: OnceLock<RealtimePresenceService> = OnceLock::new();
        INSTANCE.get_or_init(RealtimePresenceService::new)
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
