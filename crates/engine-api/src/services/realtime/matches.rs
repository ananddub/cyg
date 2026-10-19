//! Realtime Match Service (Relay Matches): complete lifecycle, relay data, and introspections.
//!
//! All state and methods are fully encapsulated on the `RealtimeMatchService` struct as lock-free `FastMap`s,
//! injected as a singleton via `auto_di` with zero global statics or free functions.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use auto_di::singleton;
use auto_ws::{Conn, Msg};
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage, Presence};
use crate::infra::ws::session::{self, Session};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of};
use super::{FastMap, new_fast_map};

pub struct Match {
    pub id: Uuid,
    pub label: String,
    pub open: bool,
    pub tick_rate: i32,
    pub members: HashMap<Uuid /*sid*/, Presence>,
    pub tokens: HashSet<String>,
    pub ephemeral: bool,
}

pub struct MatchSnapshot {
    pub id: Uuid,
    pub label: String,
    pub open: bool,
    pub tick_rate: i32,
    pub members: Vec<Presence>,
}

pub struct RealtimeMatchService {
    pub matches: FastMap<Uuid, Match>,
    pub tokens_to_match: FastMap<String, Uuid>,
}

#[singleton]
impl RealtimeMatchService {
    pub fn new() -> Self {
        Self {
            matches: new_fast_map(),
            tokens_to_match: new_fast_map(),
        }
    }

    /// Accessor for non-DI environments (console routes, background matchmaker, tests).
    pub fn global() -> &'static RealtimeMatchService {
        static INSTANCE: OnceLock<RealtimeMatchService> = OnceLock::new();
        INSTANCE.get_or_init(RealtimeMatchService::new)
    }

    // --- Associated Static Helpers (delegate to singleton) ---

    pub fn list_snapshots_all() -> Vec<MatchSnapshot> {
        Self::global().list_snapshots()
    }

    pub fn get_snapshot(id: Uuid) -> Option<MatchSnapshot> {
        Self::global().snapshot(id)
    }

    pub fn check_match_exists(id: Uuid) -> bool {
        Self::global().match_exists(id)
    }

    pub fn create_match_with_tokens(tick_rate: i32, tokens: Vec<String>) -> Uuid {
        Self::global().create_with_tokens(tick_rate, tokens)
    }

    pub fn terminate_match(id: Uuid) -> bool {
        Self::global().terminate(id)
    }
}

impl RealtimeMatchService {
    // --- Private Helper Methods ---

    fn snapshot_of(&self, m: &Match) -> MatchSnapshot {
        MatchSnapshot {
            id: m.id,
            label: m.label.clone(),
            open: m.open,
            tick_rate: m.tick_rate,
            members: m.members.values().cloned().collect(),
        }
    }

    fn presence_of(&self, s: &Session) -> Presence {
        Presence {
            user_id: s.uid.simple().to_string(),
            session_id: s.sid.simple().to_string(),
            username: s.username.clone(),
            node: None,
        }
    }

    fn notify(&self, user_ids: &[Uuid], except_sid: Uuid, event: &Envelope) {
        let mut seen = HashSet::new();
        for uid in user_ids {
            if !seen.insert(*uid) {
                continue;
            }
            for sess in session::sessions_of(*uid) {
                if sess.sid != except_sid {
                    sess.send(event);
                }
            }
        }
    }

    // --- Query & Introspection ---

    pub fn match_exists(&self, id: Uuid) -> bool {
        self.matches.contains_sync(&id)
    }

    pub fn list_snapshots(&self) -> Vec<MatchSnapshot> {
        let mut list = Vec::new();
        self.matches.iter_sync(|_, m| {
            list.push(self.snapshot_of(m));
            true
        });
        list
    }

    pub fn snapshot(&self, id: Uuid) -> Option<MatchSnapshot> {
        self.matches.read_sync(&id, |_, m| self.snapshot_of(m))
    }

    // --- Match Lifecycle & Operations ---

    pub fn create_match(&self, _s: &Arc<Session>, open: bool, tick_rate: i32, label: String) -> Uuid {
        let id = Uuid::new_v4();
        let _ = self.matches.insert_sync(
            id,
            Match {
                id,
                label,
                open,
                tick_rate,
                members: HashMap::new(),
                tokens: HashSet::new(),
                ephemeral: false,
            },
        );
        id
    }

    pub fn create_with_tokens(&self, tick_rate: i32, tokens: Vec<String>) -> Uuid {
        let id = Uuid::new_v4();
        for tok in &tokens {
            let _ = self.tokens_to_match.insert_sync(tok.clone(), id);
        }
        let _ = self.matches.insert_sync(
            id,
            Match {
                id,
                label: String::new(),
                open: false,
                tick_rate,
                members: HashMap::new(),
                tokens: tokens.into_iter().collect(),
                ephemeral: true,
            },
        );
        id
    }

    pub fn join_match(
        &self,
        s: &Arc<Session>,
        match_id: Option<String>,
        token: Option<String>,
        label: Option<String>,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let sid = s.sid;

        let target: Uuid = if let Some(raw) = match_id.filter(|m| !m.is_empty()) {
            let id = Uuid::parse_str(&raw).map_err(|_| V2Error::not_found("match not found"))?;
            if !self.matches.contains_sync(&id) {
                return Err(V2Error::not_found("match not found"));
            }
            id
        } else if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
            self.tokens_to_match
                .read_sync(tok, |_, id| *id)
                .ok_or_else(|| V2Error::not_found("match token invalid or expired"))?
        } else if let Some(lbl) = label.filter(|l| !l.is_empty()) {
            let mut found = None;
            self.matches.iter_sync(|id, m| {
                if m.label == lbl && m.open {
                    found = Some(*id);
                    false
                } else {
                    true
                }
            });
            match found {
                Some(id) => id,
                None => {
                    let id = Uuid::new_v4();
                    let _ = self.matches.insert_sync(
                        id,
                        Match {
                            id,
                            label: lbl,
                            open: true,
                            tick_rate: 0,
                            members: HashMap::new(),
                            tokens: HashSet::new(),
                            ephemeral: false,
                        },
                    );
                    id
                }
            }
        } else {
            return Err(V2Error::invalid_argument(
                "match_join needs match_id, token or label",
            ));
        };

        let mut match_ref = self.matches.get_sync(&target).ok_or_else(|| V2Error::not_found("match not found"))?;
        if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
            match_ref.tokens.remove(tok);
            self.tokens_to_match.remove_sync(tok);
        } else if !match_ref.open || match_ref.ephemeral {
            return Err(V2Error::permission_denied("not permitted to join match"));
        }
        let mine = self.presence_of(s);
        let others: Vec<Presence> = match_ref.members.values().cloned().collect();
        match_ref.members.insert(sid, mine.clone());
        let member_ids: Vec<Uuid> = match_ref.members.keys().copied().collect();
        drop(match_ref);

        let _ = s.matches.insert_sync(target);
        s.send_message(
            cid.clone(),
            EnvelopeMessage::MatchJoin {
                match_id: Some(target.simple().to_string()),
                token: None,
                label: None,
                presences: others,
            },
        );
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchPresenceEvent {
                match_id: target.simple().to_string(),
                joins: vec![mine],
                leaves: Vec::new(),
            },
        };
        self.notify(&member_ids, sid, &event);
        Ok(())
    }

    pub fn leave_match(&self, s: &Arc<Session>, raw_id: &str, cid: Option<String>) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let (member_ids, gone) = {
            let match_ref = self.matches.get_sync(&id);
            let Some(mut m) = match_ref else {
                let _ = s.matches.remove_sync(&id);
                return Ok(());
            };
            let gone = m.members.remove(&s.sid);
            let member_ids: Vec<Uuid> = m.members.keys().copied().collect();
            let empty = m.members.is_empty() && (m.ephemeral || m.tokens.is_empty());
            drop(m);
            if empty {
                if let Some((_, removed)) = self.matches.remove_sync(&id) {
                    for tok in &removed.tokens {
                        self.tokens_to_match.remove_sync(tok);
                    }
                }
            }
            (member_ids, gone)
        };
        let _ = s.matches.remove_sync(&id);
        if let Some(presence) = gone {
            let event = Envelope {
                cid: None,
                message: EnvelopeMessage::MatchPresenceEvent {
                    match_id: id.simple().to_string(),
                    joins: Vec::new(),
                    leaves: vec![presence],
                },
            };
            self.notify(&member_ids, s.sid, &event);
        }
        s.send_message(
            cid,
            EnvelopeMessage::MatchLeave {
                match_id: raw_id.to_string(),
            },
        );
        Ok(())
    }

    pub fn data_send_match(
        &self,
        s: &Arc<Session>,
        raw_id: &str,
        op_code: i64,
        data: String,
    ) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let (recipients, sender) = self.matches.read_sync(&id, |_, m| {
            if !m.members.contains_key(&s.sid) {
                return Err(V2Error::permission_denied(
                    "must join the match before sending data",
                ));
            }
            let recipients: Vec<Uuid> = m
                .members
                .values()
                .map(|p| Uuid::parse_str(&p.user_id).unwrap_or_default())
                .collect();
            Ok((recipients, self.presence_of(s)))
        }).ok_or_else(|| V2Error::not_found("match not found"))??;
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchData {
                match_id: id.simple().to_string(),
                presence: Some(sender),
                op_code,
                data,
                reliable: true,
            },
        };
        self.notify(&recipients, s.sid, &event);
        Ok(())
    }

    pub fn signal_match(
        &self,
        s: &Arc<Session>,
        raw_id: &str,
        data: String,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let member_uids: Vec<Uuid> = self.matches.read_sync(&id, |_, m| {
            m.members
                .values()
                .map(|p| Uuid::parse_str(&p.user_id).unwrap_or_default())
                .collect()
        }).ok_or_else(|| V2Error::not_found("match not found"))?;
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchSignal {
                match_id: id.simple().to_string(),
                data,
            },
        };
        self.notify(&member_uids, s.sid, &event);
        s.send_message(
            cid,
            EnvelopeMessage::MatchSignal {
                match_id: id.simple().to_string(),
                data: String::new(),
            },
        );
        Ok(())
    }

    pub fn cleanup(&self, s: &Arc<Session>) {
        let mut ids: Vec<Uuid> = Vec::new();
        s.matches.iter_sync(|id| {
            ids.push(*id);
            true
        });
        for id in ids {
            let (member_ids, presence) = {
                let match_ref = self.matches.get_sync(&id);
                let Some(mut m) = match_ref else { continue };
                let presence = m.members.remove(&s.sid);
                let member_ids: Vec<Uuid> = m
                    .members
                    .keys()
                    .filter_map(|sid| session::find(*sid))
                    .map(|sess| sess.uid)
                    .collect();
                let empty = m.members.is_empty() && m.ephemeral;
                drop(m);
                if empty {
                    if let Some((_, removed)) = self.matches.remove_sync(&id) {
                        for tok in &removed.tokens {
                            self.tokens_to_match.remove_sync(tok);
                        }
                    }
                }
                (member_ids, presence)
            };
            if let Some(presence) = presence {
                let event = Envelope {
                    cid: None,
                    message: EnvelopeMessage::MatchPresenceEvent {
                        match_id: id.simple().to_string(),
                        joins: Vec::new(),
                        leaves: vec![presence],
                    },
                };
                for uid in member_ids {
                    session::send_to_user(uid, &event);
                }
            }
        }
    }

    pub fn terminate(&self, id: Uuid) -> bool {
        let members: Vec<Presence> = {
            let Some((_, m)) = self.matches.remove_sync(&id) else {
                return false;
            };
            for tok in &m.tokens {
                self.tokens_to_match.remove_sync(tok);
            }
            m.members.values().cloned().collect()
        };
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchPresenceEvent {
                match_id: id.simple().to_string(),
                joins: Vec::new(),
                leaves: members.clone(),
            },
        };
        for presence in &members {
            let Ok(uid) = Uuid::parse_str(&presence.user_id) else {
                continue;
            };
            for sess in session::sessions_of(uid) {
                let _ = sess.matches.remove_sync(&id);
                sess.send(&event);
            }
        }
        true
    }

    // --- Realtime WebSocket Event Handlers ---

    pub async fn create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchCreate = msg.payload()?;
        let id = self.create_match(
            &session_of(conn),
            req.open,
            req.tick_rate,
            req.name.unwrap_or(req.label),
        );
        Ok(Some(EnvelopeMessage::MatchCreateAck { match_id: id.simple().to_string() }))
    }

    pub async fn join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchJoin = msg.payload()?;
        self.join_match(
            &session_of(conn),
            req.match_id,
            req.token,
            req.label,
            msg.cid().map(str::to_string),
        )
        .map(|_| None)
    }

    pub async fn leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchLeave = msg.payload()?;
        self.leave_match(&session_of(conn), &req.match_id, msg.cid().map(str::to_string))
            .map(|_| None)
    }

    pub async fn data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchDataSend = msg.payload()?;
        self.data_send_match(&session_of(conn), &req.match_id, req.op_code, req.data)
            .map(|_| None)
    }

    pub async fn signal(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchSignal = msg.payload()?;
        self.signal_match(
            &session_of(conn),
            &req.match_id,
            req.data,
            msg.cid().map(str::to_string),
        )
        .map(|_| None)
    }
}
