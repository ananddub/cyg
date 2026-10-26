//! Realtime Presence & Status Service: online tracking, follow graphs, and status broadcasts.
//!
//! All state and methods are fully encapsulated on the `RealtimePresenceService` struct as lock-free `FastMap`s,
//! injected as a singleton via `auto_di` with zero global statics or free functions.

use auto_di::singleton;
use auto_ws::{Conn, Msg};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{FastMap, new_fast_map};
use crate::ApiState;
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, state_of};
use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence, StatusPresence};
use crate::infra::ws::session::{self, Session};

pub struct RealtimePresenceService {
    /// Online presences: uid -> (sid -> (username, status))
    ///
    /// Process-wide, because the socket plane follows through `auto_di` while
    /// teardown and the read routes reach the same state through
    /// [`Self::global`].
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

    /// Accessor for non-DI environments (session teardown, background matchmaker).
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

impl RealtimePresenceService {
    // --- Private Helper Methods ---

    fn to_status_presence(
        &self,
        uid: Uuid,
        sid: Uuid,
        username: &str,
        status: &str,
    ) -> StatusPresence {
        StatusPresence {
            user_id: uid.simple().to_string(),
            session_id: sid.simple().to_string(),
            username: username.to_string(),
            status: status.to_string(),
        }
    }

    fn followers_of(&self, target: Uuid) -> Vec<Arc<Session>> {
        let mut seen = Vec::new();
        for uid in session::online_users() {
            for s in session::sessions_of(uid) {
                if s.follows.contains_sync(&target) {
                    seen.push(s);
                }
            }
        }
        seen
    }

    // --- Query & Introspection ---

    pub fn online_for(&self, uids: &[Uuid]) -> Vec<StatusPresence> {
        uids.iter()
            .filter_map(|uid| {
                self.presences
                    .read_sync(uid, |_, by_sid| {
                        by_sid.iter().next().map(|(sid, (username, status))| {
                            self.to_status_presence(*uid, *sid, username, status)
                        })
                    })
                    .flatten()
            })
            .collect()
    }

    pub fn presence_of(&self, session: &Session) -> Presence {
        Presence {
            user_id: session.uid.simple().to_string(),
            session_id: session.sid.simple().to_string(),
            username: session.username.clone(),
            node: None,
        }
    }

    // --- Lifecycle & Operations ---

    pub fn on_disconnect(&self, sid: Uuid) {
        let Some(session) = session::find(sid) else {
            return;
        };
        let gone = {
            let presence_ref = self.presences.get_sync(&session.uid);
            let Some(mut by_sid) = presence_ref else {
                return;
            };
            by_sid.get_mut().remove(&sid);
            if by_sid.is_empty() {
                let _ = by_sid.remove();
                true
            } else {
                false
            }
        };
        if gone {
            let leaves =
                vec![self.to_status_presence(session.uid, session.sid, &session.username, "")];
            for f in self.followers_of(session.uid) {
                f.send(&Envelope {
                    cid: None,
                    message: EnvelopeMessage::StatusPresenceEvent {
                        joins: Vec::new(),
                        leaves: leaves.clone(),
                    },
                });
            }
        }
    }

    pub async fn follow_users(
        &self,
        state: &ApiState,
        sid: Uuid,
        user_ids: Vec<String>,
        usernames: Vec<String>,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let Some(session) = session::find(sid) else {
            return Ok(());
        };
        let queries = crate::services::auth::q(state)?;
        let mut resolved: Vec<Uuid> = Vec::new();
        for raw in user_ids {
            if let Ok(uid) = Uuid::parse_str(&raw) {
                resolved.push(uid);
            }
        }
        for name in usernames {
            if let Ok(user) = queries.get_user_by_username(&name).await {
                resolved.push(user.id);
            }
        }
        for uid in &resolved {
            let _ = session.follows.insert_sync(*uid);
        }
        session.send_message(
            cid.clone(),
            EnvelopeMessage::StatusFollowed {
                user_ids: resolved.iter().map(|u| u.simple().to_string()).collect(),
            },
        );
        let joins = self.online_for(&resolved);
        if !joins.is_empty() {
            session.send(&Envelope {
                cid: None,
                message: EnvelopeMessage::StatusPresenceEvent {
                    joins,
                    leaves: Vec::new(),
                },
            });
        }
        Ok(())
    }

    pub fn unfollow_users(&self, sid: Uuid, user_ids: Vec<String>, cid: Option<String>) {
        let Some(session) = session::find(sid) else {
            return;
        };
        for raw in user_ids {
            if let Ok(uid) = Uuid::parse_str(&raw) {
                let _ = session.follows.remove_sync(&uid);
            }
        }
        session.send_message(
            cid,
            EnvelopeMessage::StatusFollowed {
                user_ids: Vec::new(),
            },
        );
    }

    pub fn update_status(&self, sid: Uuid, status: String) {
        let Some(session) = session::find(sid) else {
            return;
        };
        {
            let mut entry = self.presences.entry_sync(session.uid).or_default();
            entry
                .get_mut()
                .insert(session.sid, (session.username.clone(), status.clone()));
        }
        let followers = self.followers_of(session.uid);
        let presence =
            self.to_status_presence(session.uid, session.sid, &session.username, &status);
        for f in followers {
            if f.sid == sid {
                continue;
            }
            let message = EnvelopeMessage::StatusPresenceEvent {
                joins: vec![presence.clone()],
                leaves: Vec::new(),
            };
            // The caller's cid belongs to its own request: a fan-out to other
            // sockets carries no correlation id, like every other event here.
            f.send(&Envelope { cid: None, message });
        }
        for own in session::sessions_of(session.uid) {
            if own.sid != sid {
                own.send_message(
                    None,
                    EnvelopeMessage::StatusPresenceEvent {
                        joins: vec![presence.clone()],
                        leaves: Vec::new(),
                    },
                );
            }
        }
    }

    // --- Realtime WebSocket Event Handlers ---

    pub async fn follow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusFollow = msg.payload()?;
        self.follow_users(
            &state_of(conn),
            conn.id(),
            req.user_ids,
            req.usernames,
            msg.cid().map(str::to_string),
        )
        .await
        .map(|_| None)
    }

    pub async fn unfollow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusUnfollow = msg.payload()?;
        self.unfollow_users(conn.id(), req.user_ids, msg.cid().map(str::to_string));
        Ok(None)
    }

    pub async fn update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: StatusUpdate = msg.payload()?;
        self.update_status(conn.id(), req.status);
        Ok(None)
    }
}
