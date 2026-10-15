//! Session manager: one [`Session`] per live socket, in memory only.
//!
//! BaaS parity: `server/session.go` + `present_session.go`. A user may hold several
//! sockets; the registry is process-global so the matchmaker, parties, chat and status
//! planes can reach a session without threading `ApiState` through every callback.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::envelope::Envelope;

#[derive(Debug)]
pub struct Session {
    pub sid: Uuid,
    pub uid: Uuid,
    pub username: String,
    /// Writer task inbox: serialised envelopes, delivered in order.
    tx: UnboundedSender<String>,
    /// Match ids this socket joined (realtime presence bookkeeping).
    pub matches: RwLock<HashSet<Uuid>>,
    /// Channel ids (hex stream ids) this socket joined.
    pub channels: RwLock<HashSet<String>>,
    /// Party the socket belongs to, if any.
    pub party: RwLock<Option<Uuid>>,
    /// User ids this socket follows via `status_follow`.
    pub follows: RwLock<HashSet<Uuid>>,
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
            matches: RwLock::new(HashSet::new()),
            channels: RwLock::new(HashSet::new()),
            party: RwLock::new(None),
            follows: RwLock::new(HashSet::new()),
        })
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

#[derive(Default)]
struct Registry {
    by_sid: HashMap<Uuid, Arc<Session>>,
    by_user: HashMap<Uuid, Vec<Arc<Session>>>,
}

static HUB: std::sync::OnceLock<RwLock<Registry>> = std::sync::OnceLock::new();

fn hub() -> &'static RwLock<Registry> {
    HUB.get_or_init(|| RwLock::new(Registry::default()))
}

pub fn register(session: Arc<Session>) {
    let mut reg = hub().write().expect("session hub lock");
    reg.by_user
        .entry(session.uid)
        .or_default()
        .push(session.clone());
    reg.by_sid.insert(session.sid, session);
}

/// Drop a socket from the registry. The caller (read-loop teardown) still holds its
/// `Arc` long enough to broadcast the departure events.
pub fn unregister(sid: Uuid) -> Option<Arc<Session>> {
    let mut reg = hub().write().expect("session hub lock");
    let session = reg.by_sid.remove(&sid)?;
    if let Some(list) = reg.by_user.get_mut(&session.uid) {
        list.retain(|s| s.sid != sid);
        if list.is_empty() {
            reg.by_user.remove(&session.uid);
        }
    }
    Some(session)
}

pub fn find(sid: Uuid) -> Option<Arc<Session>> {
    hub()
        .read()
        .expect("session hub lock")
        .by_sid
        .get(&sid)
        .cloned()
}

pub fn sessions_of(uid: Uuid) -> Vec<Arc<Session>> {
    hub()
        .read()
        .expect("session hub lock")
        .by_user
        .get(&uid)
        .cloned()
        .unwrap_or_default()
}

pub fn is_online(uid: Uuid) -> bool {
    !sessions_of(uid).is_empty()
}

pub fn online_users() -> Vec<Uuid> {
    hub()
        .read()
        .expect("session hub lock")
        .by_user
        .keys()
        .copied()
        .collect()
}

/// Deliver one message to every live socket of a user.
pub fn send_to_user(uid: Uuid, env: &Envelope) {
    for s in sessions_of(uid) {
        s.send(env);
    }
}
