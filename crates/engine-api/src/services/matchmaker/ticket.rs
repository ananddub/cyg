//! Ticket lifecycle: enqueue, cancel, and socket-teardown cleanup.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use super::sweep::spawn_tick;
use super::{pool, Ticket};
use crate::infra::ws::envelope::{EnvelopeMessage, ErrorMsg};
use crate::infra::ws::session;

pub fn add(
    s: &Arc<session::Session>,
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
    let ticket = Ticket {
        id: Uuid::new_v4(),
        sid: s.sid,
        uid: s.uid,
        username: s.username.clone(),
        min_count,
        max_count,
        query,
        string_props,
        numeric_props,
        party_id: *s.party.read().expect("party lock"),
        created_at: chrono::Utc::now(),
    };
    let id = ticket.id.simple().to_string();
    pool().write().expect("matchmaker pool lock").push(ticket);
    spawn_tick();
    Ok(id)
}

pub fn remove(s: &Arc<session::Session>, ticket: &str) -> bool {
    let Ok(id) = Uuid::parse_str(ticket) else {
        return false;
    };
    let mut all = pool().write().expect("matchmaker pool lock");
    let before = all.len();
    all.retain(|t| !(t.id == id && t.sid == s.sid));
    all.len() != before
}

/// Socket teardown: drop every ticket held by the session.
pub fn drop_tickets(sid: Uuid) {
    pool()
        .write()
        .expect("matchmaker pool lock")
        .retain(|t| t.sid != sid);
}
