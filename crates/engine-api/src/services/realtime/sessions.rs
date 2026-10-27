//! Realtime Session Hub using lock-free `scc::HashMap` and `ahash`.
//!
//! Replaces legacy global `OnceLock<RwLock<Registry>>` with a lock-free `#[singleton]`.

use auto_di::singleton;
use std::sync::{Arc, OnceLock};
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
    /// Accessor for non-DI environments (the console ban path), the same
    /// pattern [`super::presence::RealtimePresenceService::global`] uses.
    pub fn global() -> &'static RealtimeSessionHub {
        static INSTANCE: OnceLock<RealtimeSessionHub> = OnceLock::new();
        INSTANCE.get_or_init(RealtimeSessionHub::new)
    }

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

    /// Kick every live socket of one user: each joined match is left properly
    /// (the other members see the presence leave), an error frame names the
    /// reason, and the socket is closed with every registry forgetting it.
    /// Returns how many sockets went. The console ban path calls this right
    /// after revoking the user's tokens, so a banned player leaves realtime
    /// instead of at their next login.
    pub fn kick_user(&self, uid: Uuid, reason: &str) -> usize {
        let sessions = self.sessions_of(uid);
        for session in &sessions {
            let mut joined: Vec<Uuid> = Vec::new();
            session.matches.iter_sync(|id| {
                joined.push(*id);
                true
            });
            for id in joined {
                let _ = crate::services::realtime::RealtimeMatchService::global()
                    .leave_match(session, &id.simple().to_string(), None);
            }
            session.send_message(
                None,
                crate::infra::ws::envelope::EnvelopeMessage::Error(
                    crate::infra::ws::envelope::ErrorMsg {
                        message: reason.to_owned(),
                    },
                ),
            );
            crate::services::realtime::RealtimePresenceService::handle_disconnect(session.sid);
            self.unregister(session.sid);
            session.close();
        }
        sessions.len()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::ws::envelope::Envelope;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    /// One live socket registered under `uid`, with its reader end kept for
    /// asserting what the kick delivered.
    fn registered_socket(uid: Uuid, name: &str) -> (Arc<Session>, UnboundedReceiver<String>) {
        let (tx, rx) = unbounded_channel::<String>();
        let session = Session::new(Uuid::new_v4(), uid, name.to_owned(), tx);
        RealtimeSessionHub::global().register(session.clone());
        (session, rx)
    }

    #[test]
    fn kick_user_forgets_the_sessions_and_closes_the_sockets() {
        let uid = Uuid::new_v4();
        let (session_a, mut rx_a) = registered_socket(uid, "a");
        let (_session_b, _rx_b) = registered_socket(uid, "b");
        let (stranger, _rx_stranger) = registered_socket(Uuid::new_v4(), "stranger");

        let kicked = RealtimeSessionHub::global().kick_user(uid, "Banned: test");
        assert_eq!(kicked, 2, "both sockets of the user go");
        assert!(
            RealtimeSessionHub::global().sessions_of(uid).is_empty(),
            "the registry forgets the user"
        );
        // A different user's socket is untouched.
        assert_eq!(RealtimeSessionHub::global().sessions_of(stranger.uid).len(), 1);

        // Each kicked socket first sees the error frame naming the reason, then
        // the channel ends (the close swap dropped the live sender).
        for rx in [&mut rx_a] {
            let frame = rx.blocking_recv().expect("the error frame arrives");
            assert!(frame.contains("\"error\""), "frame: {frame}");
            assert!(frame.contains("Banned"), "frame: {frame}");
            assert!(rx.blocking_recv().is_none(), "the socket is closed after the frame");
        }
        let _ = session_a;
    }

    #[test]
    fn kick_user_leaves_every_match_with_the_presence_event() {
        let uid = Uuid::new_v4();
        let (session, mut rx) = registered_socket(uid, "member");
        let matches = crate::services::realtime::RealtimeMatchService::global();
        let match_id = matches.create_match(&session, true, 30, "kick-test".into());
        matches
            .join_match(&session, Some(match_id.to_string()), None, None, None)
            .expect("the join lands");
        assert!(matches.match_exists(match_id));

        // A second member stays in the match and is the one the presence
        // leave event rides to.
        let (bystander, mut rx_bystander) = registered_socket(Uuid::new_v4(), "bystander");
        matches
            .join_match(&bystander, Some(match_id.to_string()), None, None, None)
            .expect("the second join lands");

        let kicked = RealtimeSessionHub::global().kick_user(uid, "Banned");
        assert_eq!(kicked, 1);

        // The leave ran: the kicked member is gone from the match, and an
        // empty match is retired by the same rule a manual leave follows —
        // `get_snapshot` returning None here is the correct outcome too.
        if let Some(snapshot) = crate::services::realtime::RealtimeMatchService::get_snapshot(match_id) {
            assert!(
                snapshot.members.iter().all(|m| m.user_id != uid.to_string()),
                "the kicked member is gone: {:?}",
                snapshot.members
            );
        }
        // The kicked socket saw its join and leave confirmations, then the ban
        // error, then the channel ends.
        let frames: Vec<String> = std::iter::from_fn(|| rx.blocking_recv()).collect();
        assert!(
            frames.iter().any(|f| f.contains("\"error\"")),
            "the ban error frame rides: {frames:?}"
        );
        // The bystander saw the kicked member's presence leave. Its socket is
        // still alive, so the read is bounded: join ack, then the leave event.
        let seen: Vec<String> =
            (0..2).filter_map(|_| rx_bystander.blocking_recv()).collect();
        assert!(
            seen.iter().any(|f| f.contains("match_presence") && f.contains("leaves")),
            "the presence leave rides to the remaining member: {seen:?}"
        );
    }

    #[test]
    fn kick_of_an_offline_user_is_a_quiet_zero() {
        let uid = Uuid::new_v4();
        assert_eq!(RealtimeSessionHub::global().kick_user(uid, "Banned"), 0);
        // And an envelope dropped into a closed session is lost, not panic'd on.
        let (tx, rx) = unbounded_channel::<String>();
        let session = Session::new(Uuid::new_v4(), uid, "dying".into(), tx);
        session.close();
        drop(rx);
        session.send(&Envelope { cid: None, message: crate::infra::ws::envelope::EnvelopeMessage::Ping });
    }
}
