//! Realtime Session Hub using lock-free `scc::HashMap` and `ahash`.
//!
//! Replaces legacy global `OnceLock<RwLock<Registry>>` with a lock-free `#[singleton]`.

use auto_di::singleton;
use std::sync::Arc;
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::infra::ws::envelope::Envelope;
use crate::infra::ws::session::Session;

pub struct RealtimeSessionHub {
    pub by_sid: &'static FastMap<Uuid, Arc<Session>>,
    pub by_user: &'static FastMap<Uuid, Vec<Arc<Session>>>,
}

#[singleton]
impl RealtimeSessionHub {
    pub fn new() -> Self {
        static BY_SID: std::sync::OnceLock<FastMap<Uuid, Arc<Session>>> =
            std::sync::OnceLock::new();
        static BY_USER: std::sync::OnceLock<FastMap<Uuid, Vec<Arc<Session>>>> =
            std::sync::OnceLock::new();
        Self {
            by_sid: BY_SID.get_or_init(new_fast_map),
            by_user: BY_USER.get_or_init(new_fast_map),
        }
    }
}

impl RealtimeSessionHub {
    pub fn register(&self, session: Arc<Session>) {
        let _ = self.by_sid.insert_sync(session.sid, session.clone());
        let mut entry = self.by_user.entry_sync(session.uid).or_default();
        entry.push(session);
    }

    pub fn unregister(&self, sid: Uuid) -> Option<Arc<Session>> {
        let (_, session) = self.by_sid.remove_sync(&sid)?;
        if let Some(mut entry) = self.by_user.get_sync(&session.uid) {
            entry.retain(|s| s.sid != sid);
            if entry.is_empty() {
                drop(entry);
                let _ = self.by_user.remove_sync(&session.uid);
            }
        }
        Some(session)
    }

    pub fn find(&self, sid: Uuid) -> Option<Arc<Session>> {
        self.by_sid.read_sync(&sid, |_, s| s.clone())
    }

    pub fn sessions_of(&self, uid: Uuid) -> Vec<Arc<Session>> {
        self.by_user
            .read_sync(&uid, |_, list| list.clone())
            .unwrap_or_default()
    }

    pub fn is_online(&self, uid: Uuid) -> bool {
        self.by_user
            .read_sync(&uid, |_, list| !list.is_empty())
            .unwrap_or(false)
    }

    pub fn online_users(&self) -> Vec<Uuid> {
        let mut users = Vec::new();
        self.by_user.iter_sync(|uid, _| {
            users.push(*uid);
            true
        });
        users
    }

    pub fn send_to_user(&self, uid: Uuid, env: &Envelope) {
        for s in self.sessions_of(uid) {
            s.send(env);
        }
    }
}
