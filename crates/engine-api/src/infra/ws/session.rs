//! Session manager: one [`Session`] per live socket, in memory only.
//!
//! BaaS parity: `server/session.go` + `present_session.go`. A user may hold several
//! sockets; the registry is process-global so the matchmaker, parties, chat and status
//! planes can reach a session without threading `ApiState` through every callback.

use arc_swap::ArcSwapOption;
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::envelope::Envelope;
use crate::services::realtime::{FastSet, new_fast_set};

#[derive(Debug)]
pub struct Session {
    pub sid: Uuid,
    pub uid: Uuid,
    pub username: String,
    /// Writer task inbox: serialised envelopes, delivered in order.
    tx: UnboundedSender<String>,
    /// Match ids this socket joined (realtime presence bookkeeping).
    pub matches: FastSet<Uuid>,
    /// Channel ids (hex stream ids) this socket joined.
    pub channels: FastSet<String>,
    /// Party the socket belongs to, if any.
    pub party: ArcSwapOption<Uuid>,
    /// User ids this socket follows via `status_follow`.
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
            tx,
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

    /// Queue one envelope for delivery. A full/closed queue means the writer died with
    /// the socket; the read side tears the session down on the next send.
    pub fn send(&self, env: &Envelope) {
        if let Ok(json) = serde_json::to_string(env) {
            let _ = self.tx.send(json);
        }
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

/// Drop a socket from the registry. The caller (read-loop teardown) still holds its
/// `Arc` long enough to broadcast the departure events.
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

/// Deliver one message to every live socket of a user.
pub fn send_to_user(uid: Uuid, env: &Envelope) {
    hub().send_to_user(uid, env);
}
