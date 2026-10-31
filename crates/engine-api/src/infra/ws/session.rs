use arc_swap::ArcSwapOption;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use super::envelope::Envelope;
use crate::services::realtime::{FastSet, new_fast_set};

#[derive(Debug)]
pub struct Session {
    pub sid: Uuid,
    pub uid: Uuid,
    pub username: String,
    tx: Mutex<UnboundedSender<String>>,
    pub matches: FastSet<Uuid>,
    pub channels: FastSet<String>,
    pub party: ArcSwapOption<Uuid>,
    pub follows: FastSet<Uuid>,
}

impl Session {
    pub fn new(
        sid: Uuid,
        uid: Uuid,
        username: String,
        tx: UnboundedSender<String>,
    ) -> Arc<Session> {
        Arc::new(Session {
            sid,
            uid,
            username,
            tx: Mutex::new(tx),
            matches: new_fast_set(),
            channels: new_fast_set(),
            party: ArcSwapOption::new(None),
            follows: new_fast_set(),
        })
    }

    pub fn party_id(&self) -> Option<Uuid> {
        self.party.load().as_ref().map(|u| **u)
    }

    pub fn set_party_id(&self, id: Option<Uuid>) {
        self.party.store(id.map(Arc::new));
    }

    pub fn send(&self, env: &Envelope) {
        if let Ok(json) = serde_json::to_string(env) {
            let _ = self.tx.lock().unwrap().send(json);
        }
    }

    pub fn close(&self) {
        let (dead, _dead_rx): (UnboundedSender<String>, UnboundedReceiver<String>) =
            unbounded_channel();
        *self.tx.lock().unwrap() = dead;
    }

    pub fn send_message(&self, cid: Option<String>, message: super::envelope::EnvelopeMessage) {
        self.send(&Envelope { cid, message });
    }
}

static HUB: std::sync::OnceLock<crate::services::realtime::RealtimeSessionHub> =
    std::sync::OnceLock::new();

pub fn hub() -> &'static crate::services::realtime::RealtimeSessionHub {
    HUB.get_or_init(crate::services::realtime::RealtimeSessionHub::new)
}

pub fn register(session: Arc<Session>) {
    hub().register(session);
}

pub fn unregister(sid: Uuid) -> Option<Arc<Session>> {
    hub().unregister(sid)
}

pub fn find(sid: Uuid) -> Option<Arc<Session>> {
    hub().find(sid)
}

pub fn sessions_of(uid: Uuid) -> Vec<Arc<Session>> {
    hub().sessions_of(uid)
}

pub fn is_online(uid: Uuid) -> bool {
    hub().is_online(uid)
}

pub fn online_users() -> Vec<Uuid> {
    hub().online_users()
}

pub fn send_to_user(uid: Uuid, env: &Envelope) {
    hub().send_to_user(uid, env);
}
