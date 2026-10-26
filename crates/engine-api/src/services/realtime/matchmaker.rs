//! Realtime Matchmaker Service: ticket pooling, greedy matching, and queue stats.
//!
//! All state and methods are fully encapsulated on the `RealtimeMatchmakerService` struct as lock-free `FastMap`s,
//! injected as a singleton via `auto_di` with zero global statics or free functions.

use auto_di::singleton;
use auto_ws::{Conn, Msg};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use super::{
    FastMap, FastTree, RealtimeMatchService, RealtimePresenceService, new_fast_map, new_fast_tree,
};
use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of, unwrap_env_err};
use crate::error::V2Error;
use crate::infra::ws::envelope::{
    EnvelopeMessage, ErrorMsg, MatchmakerAdd, MatchmakerMatched, MatchmakerUser, Presence,
};
use crate::infra::ws::session::{self, Session};
use scc::Guard;

static TICKER_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Debug, PartialEq)]
pub enum NumericCmp {
    Gte(f64),
    Lte(f64),
    Gt(f64),
    Lt(f64),
    Eq(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TermMatcher {
    Numeric(NumericCmp),
    Exact(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTerm {
    pub key: String,
    pub negated: bool,
    pub matcher: TermMatcher,
}

#[derive(Clone)]
pub struct Ticket {
    pub id: Uuid,
    pub sid: Uuid,
    pub uid: Uuid,
    pub username: String,
    pub min_count: i32,
    pub max_count: i32,
    pub query: String,
    pub compiled_terms: Vec<CompiledTerm>,
    pub string_props: HashMap<String, String>,
    pub numeric_props: HashMap<String, f64>,
    pub party_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct MatchsetStats {
    pub matchset: String,
    pub ticket_count: i64,
    pub oldest_create_time: DateTime<Utc>,
    pub avg_wait_secs: f64,
}

pub struct MatchmakerStatsSnapshot {
    pub ticket_count: i64,
    pub oldest_ticket_create_time: Option<DateTime<Utc>>,
    pub matchsets: Vec<MatchsetStats>,
}

pub struct RealtimeMatchmakerService {
    /// One process-wide set of indexes: the socket adds tickets through `auto_di`
    /// and the ticker, the REST stats route and the party plane read them through
    /// [`Self::global`].
    pub tickets: &'static FastMap<Uuid, Ticket>,
    pub mode_index: &'static FastMap<String, FastMap<Uuid, ()>>,
    pub rank_index: &'static FastTree<(i64, Uuid), ()>,
}

#[singleton]
impl RealtimeMatchmakerService {
    pub fn new() -> Self {
        static TICKETS: OnceLock<FastMap<Uuid, Ticket>> = OnceLock::new();
        static MODE_INDEX: OnceLock<FastMap<String, FastMap<Uuid, ()>>> = OnceLock::new();
        static RANK_INDEX: OnceLock<FastTree<(i64, Uuid), ()>> = OnceLock::new();
        Self {
            tickets: TICKETS.get_or_init(new_fast_map),
            mode_index: MODE_INDEX.get_or_init(new_fast_map),
            rank_index: RANK_INDEX.get_or_init(new_fast_tree),
        }
    }

    /// Accessor for non-DI callers (rest routes, party service).
    pub fn global() -> &'static RealtimeMatchmakerService {
        static INSTANCE: OnceLock<RealtimeMatchmakerService> = OnceLock::new();
        INSTANCE.get_or_init(RealtimeMatchmakerService::new)
    }

    pub fn add_ticket(
        s: &Arc<Session>,
        min_count: i32,
        max_count: i32,
        query: String,
        string_props: HashMap<String, String>,
        numeric_props: HashMap<String, f64>,
    ) -> Result<String, EnvelopeMessage> {
        Self::global().add_ticket_internal(
            s,
            min_count,
            max_count,
            query,
            string_props,
            numeric_props,
        )
    }

    pub fn remove_ticket(s: &Arc<Session>, ticket: &str) -> bool {
        Self::global().remove_ticket_internal(s, ticket)
    }

    pub fn drop_all_tickets(sid: Uuid) {
        Self::global().drop_tickets(sid);
    }

    pub fn get_stats() -> MatchmakerStatsSnapshot {
        Self::global().stats()
    }
}

impl RealtimeMatchmakerService {
    // --- Pre-Compiled Query Engine & Compatibility Matcher ---

    fn split_operator<'a>(&self, s: &'a str) -> (&'a str, &'a str) {
        if s.starts_with(">=") {
            (">=", &s[2..])
        } else if s.starts_with("<=") {
            ("<=", &s[2..])
        } else if s.starts_with('>') {
            (">", &s[1..])
        } else if s.starts_with('<') {
            ("<", &s[1..])
        } else {
            ("==", s)
        }
    }

    pub fn compile_query(&self, query: &str) -> Vec<CompiledTerm> {
        let mut terms = Vec::new();
        for term in query.split_whitespace() {
            let Some(colon) = term.find(':') else {
                continue;
            };
            let (lhs, value) = (&term[..colon], &term[colon + 1..]);
            let negated = lhs.starts_with('-');
            let key = lhs
                .trim_start_matches(['+', '-'])
                .strip_prefix("properties.")
                .unwrap_or("")
                .to_string();
            if value.is_empty() || key.is_empty() {
                continue;
            }
            if let Some(oppos) = value.find(|c| matches!(c, '>' | '<')) {
                let (cmp_op, rhs) = self.split_operator(&value[oppos..]);
                let Ok(target) = rhs.trim().parse::<f64>() else {
                    continue;
                };
                let cmp = match cmp_op {
                    ">=" => NumericCmp::Gte(target),
                    "<=" => NumericCmp::Lte(target),
                    ">" => NumericCmp::Gt(target),
                    "<" => NumericCmp::Lt(target),
                    _ => NumericCmp::Eq(target),
                };
                terms.push(CompiledTerm {
                    key,
                    negated,
                    matcher: TermMatcher::Numeric(cmp),
                });
            } else {
                terms.push(CompiledTerm {
                    key,
                    negated,
                    matcher: TermMatcher::Exact(value.to_string()),
                });
            }
        }
        terms
    }

    pub fn is_compatible(
        &self,
        terms: &[CompiledTerm],
        strings: &HashMap<String, String>,
        numerics: &HashMap<String, f64>,
    ) -> bool {
        for term in terms {
            match &term.matcher {
                TermMatcher::Numeric(cmp) => {
                    let actual = numerics
                        .get(&term.key)
                        .copied()
                        .or_else(|| strings.get(&term.key).and_then(|v| v.parse::<f64>().ok()));
                    let Some(val) = actual else {
                        if term.negated {
                            return false;
                        }
                        continue;
                    };
                    let ok = match cmp {
                        NumericCmp::Gte(target) => val >= *target,
                        NumericCmp::Lte(target) => val <= *target,
                        NumericCmp::Gt(target) => val > *target,
                        NumericCmp::Lt(target) => val < *target,
                        NumericCmp::Eq(target) => val == *target,
                    };
                    if term.negated {
                        if ok {
                            return false;
                        }
                    } else if !ok {
                        return false;
                    }
                }
                TermMatcher::Exact(target_str) => {
                    let equal = strings.get(&term.key).map(|s| s.as_str())
                        == Some(target_str.as_str())
                        || numerics.get(&term.key).map(|n| n.to_string()).as_deref()
                            == Some(target_str.as_str());
                    if term.negated {
                        if equal {
                            return false;
                        }
                    } else if !equal {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn query_matches(
        &self,
        query: &str,
        strings: &HashMap<String, String>,
        numerics: &HashMap<String, f64>,
    ) -> bool {
        let terms = self.compile_query(query);
        self.is_compatible(&terms, strings, numerics)
    }

    fn spawn_tick(&self) {
        TICKER_STARTED.get_or_init(|| {
            tokio::spawn(async {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    Self::global().try_match();
                }
            });
        });
    }

    // --- Indexing & Query Term Language Helpers ---

    pub fn extract_mode(&self, ticket: &Ticket) -> String {
        if let Some(m) = ticket
            .string_props
            .get("mode")
            .or_else(|| ticket.string_props.get("game_mode"))
        {
            if !m.is_empty() {
                return m.clone();
            }
        }
        for term in ticket.query.split_whitespace() {
            if let Some(rest) = term.strip_prefix("+properties.mode:") {
                let clean = rest.trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
            if let Some(rest) = term.strip_prefix("+properties.game_mode:") {
                let clean = rest.trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
        "default".to_string()
    }

    fn index_ticket(&self, ticket: &Ticket) {
        let mode = self.extract_mode(ticket);
        if let Some(bucket) = self.mode_index.get_sync(&mode) {
            let _ = bucket.insert_sync(ticket.id, ());
        } else {
            let bucket = new_fast_map();
            let _ = bucket.insert_sync(ticket.id, ());
            let _ = self.mode_index.insert_sync(mode, bucket);
        }

        if let Some(rank) = ticket
            .numeric_props
            .get("rank")
            .or_else(|| ticket.numeric_props.get("mmr"))
        {
            let _ = self.rank_index.insert_sync((*rank as i64, ticket.id), ());
        }
    }

    fn unindex_ticket(&self, ticket: &Ticket) {
        let mode = self.extract_mode(ticket);
        if let Some(bucket) = self.mode_index.get_sync(&mode) {
            bucket.remove_sync(&ticket.id);
        }
        if let Some(rank) = ticket
            .numeric_props
            .get("rank")
            .or_else(|| ticket.numeric_props.get("mmr"))
        {
            let _ = self.rank_index.remove_sync(&((*rank as i64), ticket.id));
        }
    }

    pub fn find_tickets_in_rank_range(&self, min_rank: i64, max_rank: i64) -> Vec<Uuid> {
        let guard = Guard::new();
        self.rank_index
            .range((min_rank, Uuid::nil())..=(max_rank, Uuid::max()), &guard)
            .map(|((_, id), _)| *id)
            .collect()
    }

    // --- Ticket Operations ---

    pub fn add_ticket_internal(
        &self,
        s: &Arc<Session>,
        min_count: i32,
        max_count: i32,
        query: String,
        string_props: HashMap<String, String>,
        numeric_props: HashMap<String, f64>,
    ) -> Result<String, EnvelopeMessage> {
        if !(2..=8).contains(&min_count) || !(2..=8).contains(&max_count) || min_count > max_count {
            return Err(EnvelopeMessage::Error(ErrorMsg {
                message: "min_count and max_count must be between 2 and 8, and min <= max".into(),
            }));
        }
        let compiled_terms = self.compile_query(&query);
        let ticket = Ticket {
            id: Uuid::new_v4(),
            sid: s.sid,
            uid: s.uid,
            username: s.username.clone(),
            min_count,
            max_count,
            query,
            compiled_terms,
            string_props,
            numeric_props,
            party_id: s.party_id(),
            created_at: Utc::now(),
        };
        let id_str = ticket.id.simple().to_string();
        self.index_ticket(&ticket);
        let _ = self.tickets.insert_sync(ticket.id, ticket);
        self.spawn_tick();
        Ok(id_str)
    }

    pub fn remove_ticket_internal(&self, s: &Arc<Session>, ticket: &str) -> bool {
        let Ok(id) = Uuid::parse_str(ticket) else {
            return false;
        };
        let should_remove = self
            .tickets
            .read_sync(&id, |_, t| t.sid == s.sid)
            .unwrap_or(false);
        if should_remove {
            if let Some((_, t)) = self.tickets.remove_sync(&id) {
                self.unindex_ticket(&t);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn drop_tickets(&self, sid: Uuid) {
        let mut to_remove = Vec::new();
        self.tickets.iter_sync(|id, t| {
            if t.sid == sid {
                to_remove.push(*id);
            }
            true
        });
        for id in to_remove {
            if let Some((_, t)) = self.tickets.remove_sync(&id) {
                self.unindex_ticket(&t);
            }
        }
    }

    pub fn stats(&self) -> MatchmakerStatsSnapshot {
        let now = Utc::now();
        let mut all: Vec<Ticket> = Vec::new();
        self.tickets.iter_sync(|_, t| {
            all.push(t.clone());
            true
        });
        let mut oldest: Option<DateTime<Utc>> = None;
        let mut groups: HashMap<String, Vec<i64>> = HashMap::new();
        for ticket in &all {
            let waited = (now - ticket.created_at).num_milliseconds();
            oldest = Some(oldest.map_or(ticket.created_at, |o| o.min(ticket.created_at)));
            groups.entry(ticket.query.clone()).or_default().push(waited);
        }
        let mut matchsets: Vec<MatchsetStats> = groups
            .into_iter()
            .map(|(matchset, waits)| MatchsetStats {
                matchset,
                ticket_count: waits.len() as i64,
                oldest_create_time: now
                    - chrono::Duration::milliseconds(waits.iter().copied().min().unwrap_or(0)),
                avg_wait_secs: waits.iter().copied().sum::<i64>() as f64
                    / waits.len() as f64
                    / 1000.0,
            })
            .collect();
        matchsets.sort_by(|a, b| a.matchset.cmp(&b.matchset));
        MatchmakerStatsSnapshot {
            ticket_count: all.len() as i64,
            oldest_ticket_create_time: oldest,
            matchsets,
        }
    }

    pub fn try_match(&self) {
        if self.tickets.is_empty() {
            return;
        }

        let mut modes: Vec<String> = Vec::new();
        self.mode_index.iter_sync(|mode, _| {
            modes.push(mode.clone());
            true
        });

        if modes.is_empty() {
            return;
        }

        let mut all_formed: Vec<Vec<Ticket>> = Vec::new();
        let mut all_used: HashSet<Uuid> = HashSet::new();

        for mode in modes {
            let Some(bucket) = self.mode_index.get_sync(&mode) else {
                continue;
            };
            let mut tickets: Vec<Ticket> = Vec::new();
            bucket.iter_sync(|id, _| {
                if let Some(t) = self.tickets.read_sync(id, |_, t| t.clone()) {
                    tickets.push(t);
                }
                true
            });

            if tickets.len() < 2 {
                continue;
            }

            // Oldest-first ordering inside this mode bucket
            tickets.sort_by_key(|t| t.created_at);

            let mut used: HashSet<Uuid> = HashSet::new();
            for seed in &tickets {
                if used.contains(&seed.id) {
                    continue;
                }
                let mut group: Vec<Ticket> = vec![seed.clone()];
                used.insert(seed.id);
                for candidate in &tickets {
                    if group.len() as i32 >= seed.max_count {
                        break;
                    }
                    if used.contains(&candidate.id) {
                        continue;
                    }
                    let compatible = group.iter().all(|g| {
                        self.is_compatible(
                            &g.compiled_terms,
                            &candidate.string_props,
                            &candidate.numeric_props,
                        ) && self.is_compatible(
                            &candidate.compiled_terms,
                            &g.string_props,
                            &g.numeric_props,
                        )
                    }) && (group.len() as i32 + 1) <= candidate.max_count;
                    if compatible {
                        group.push(candidate.clone());
                        used.insert(candidate.id);
                    }
                }
                if group.len() as i32 >= group.iter().map(|t| t.min_count).max().unwrap_or(99) {
                    all_formed.push(group);
                } else {
                    for t in &group {
                        used.remove(&t.id);
                    }
                }
            }
            all_used.extend(used);
        }

        if all_formed.is_empty() {
            return;
        }

        for t_id in &all_used {
            if let Some((_, t)) = self.tickets.remove_sync(t_id) {
                self.unindex_ticket(&t);
            }
        }

        for group in all_formed {
            let tokens: Vec<String> = group
                .iter()
                .map(|_| Uuid::new_v4().simple().to_string())
                .collect();
            let match_id = RealtimeMatchService::create_match_with_tokens(0, tokens.clone());
            for (i, member) in group.iter().enumerate() {
                let Some(sess) = session::find(member.sid) else {
                    continue;
                };
                let users: Vec<MatchmakerUser> = group
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, other)| MatchmakerUser {
                        presence: Presence {
                            user_id: other.uid.simple().to_string(),
                            session_id: other.sid.simple().to_string(),
                            username: other.username.clone(),
                            node: None,
                        },
                        party_id: other.party_id.map(|p| p.simple().to_string()),
                        string_properties: other.string_props.clone(),
                        numeric_properties: other.numeric_props.clone(),
                    })
                    .collect();
                let self_presence = MatchmakerUser {
                    presence: RealtimePresenceService::get_presence_of(&sess),
                    party_id: member.party_id.map(|p| p.simple().to_string()),
                    string_properties: member.string_props.clone(),
                    numeric_properties: member.numeric_props.clone(),
                };
                sess.send_message(
                    None,
                    EnvelopeMessage::MatchmakerMatched(MatchmakerMatched {
                        ticket: member.id.simple().to_string(),
                        match_id: match_id.simple().to_string(),
                        token: tokens[i].clone(),
                        users,
                        self_presence,
                    }),
                );
            }
        }
    }

    // --- Realtime WebSocket Event Handlers ---

    pub async fn add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let add: MatchmakerAdd = msg.payload()?;
        self.add_ticket_internal(
            &session_of(conn),
            add.min_count,
            add.max_count,
            add.query,
            add.string_properties,
            add.numeric_properties,
        )
        .map(|ticket| Some(EnvelopeMessage::MatchmakerTicket { ticket }))
        .map_err(unwrap_env_err)
    }

    pub async fn remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchmakerRemove = msg.payload()?;
        if self.remove_ticket_internal(&session_of(conn), &req.ticket) {
            Ok(Some(EnvelopeMessage::MatchmakerDone))
        } else {
            Err(V2Error::not_found("matchmaker ticket not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props() -> (HashMap<String, String>, HashMap<String, f64>) {
        (
            HashMap::from([("mode".to_string(), "deathmatch".to_string())]),
            HashMap::from([("rank".to_string(), 42.0f64)]),
        )
    }

    #[test]
    fn empty_query_matches_anything() {
        let (s, n) = props();
        let svc = RealtimeMatchmakerService::new();
        assert!(svc.query_matches("", &s, &n));
    }

    #[test]
    fn equality_and_comparisons() {
        let (s, n) = props();
        let svc = RealtimeMatchmakerService::new();
        assert!(svc.query_matches("+properties.mode:deathmatch", &s, &n));
        assert!(!svc.query_matches("+properties.mode:capture", &s, &n));
        assert!(svc.query_matches("+properties.rank:>=40", &s, &n));
        assert!(!svc.query_matches("+properties.rank:>42", &s, &n));
        assert!(svc.query_matches("-properties.mode:capture", &s, &n));
        assert!(!svc.query_matches("-properties.mode:deathmatch", &s, &n));
    }

    #[test]
    fn mode_bucketing_and_rank_indexing() {
        let svc = RealtimeMatchmakerService::new();
        let q1 = "+properties.mode:deathmatch".to_string();
        let t1 = Ticket {
            id: Uuid::new_v4(),
            sid: Uuid::new_v4(),
            uid: Uuid::new_v4(),
            username: "player1".into(),
            min_count: 2,
            max_count: 4,
            compiled_terms: svc.compile_query(&q1),
            query: q1,
            string_props: HashMap::from([("mode".into(), "deathmatch".into())]),
            numeric_props: HashMap::from([("rank".into(), 25.0)]),
            party_id: None,
            created_at: Utc::now(),
        };
        let q2 = "+properties.mode:battle_royale".to_string();
        let t2 = Ticket {
            id: Uuid::new_v4(),
            sid: Uuid::new_v4(),
            uid: Uuid::new_v4(),
            username: "player2".into(),
            min_count: 2,
            max_count: 4,
            compiled_terms: svc.compile_query(&q2),
            query: q2,
            string_props: HashMap::from([("mode".into(), "battle_royale".into())]),
            numeric_props: HashMap::from([("rank".into(), 85.0)]),
            party_id: None,
            created_at: Utc::now(),
        };

        assert_eq!(svc.extract_mode(&t1), "deathmatch");
        assert_eq!(svc.extract_mode(&t2), "battle_royale");

        svc.index_ticket(&t1);
        svc.index_ticket(&t2);

        // Verify mode bucketing in mode_index
        assert!(
            svc.mode_index
                .get_sync("deathmatch")
                .unwrap()
                .contains_sync(&t1.id)
        );
        assert!(
            svc.mode_index
                .get_sync("battle_royale")
                .unwrap()
                .contains_sync(&t2.id)
        );

        // Verify lock-free range queries via scc::TreeIndex
        let low_rank = svc.find_tickets_in_rank_range(20, 30);
        assert_eq!(low_rank.len(), 1);
        assert_eq!(low_rank[0], t1.id);

        let high_rank = svc.find_tickets_in_rank_range(80, 90);
        assert_eq!(high_rank.len(), 1);
        assert_eq!(high_rank[0], t2.id);

        let none_rank = svc.find_tickets_in_rank_range(50, 60);
        assert!(none_rank.is_empty());

        // Unindex clears both indices
        svc.unindex_ticket(&t1);
        assert!(
            !svc.mode_index
                .get_sync("deathmatch")
                .unwrap()
                .contains_sync(&t1.id)
        );
        assert!(svc.find_tickets_in_rank_range(20, 30).is_empty());
    }

    #[test]
    fn precompiled_ast_zero_alloc_matching() {
        let svc = RealtimeMatchmakerService::new();
        let query = "+properties.mode:ranked +properties.rank:>=50 +properties.rank:<=100 -properties.banned:true";
        let terms = svc.compile_query(query);
        assert_eq!(terms.len(), 4);

        let valid_strings = HashMap::from([("mode".into(), "ranked".into())]);
        let valid_numerics = HashMap::from([("rank".into(), 75.0)]);
        assert!(svc.is_compatible(&terms, &valid_strings, &valid_numerics));

        // Too low rank
        let low_numerics = HashMap::from([("rank".into(), 40.0)]);
        assert!(!svc.is_compatible(&terms, &valid_strings, &low_numerics));

        // Too high rank
        let high_numerics = HashMap::from([("rank".into(), 120.0)]);
        assert!(!svc.is_compatible(&terms, &valid_strings, &high_numerics));

        // Banned
        let banned_strings = HashMap::from([
            ("mode".into(), "ranked".into()),
            ("banned".into(), "true".into()),
        ]);
        assert!(!svc.is_compatible(&terms, &banned_strings, &valid_numerics));
    }
}
