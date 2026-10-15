//! One sweep pass: greedy pairing over the shared ticket pool.

use std::collections::HashSet;

use uuid::Uuid;

use super::{pool, query_matches, Ticket};
use crate::infra::ws::envelope::{EnvelopeMessage, MatchmakerMatched, MatchmakerUser, Presence};
use crate::infra::ws::session;
use crate::services::{match_handler, presence};

/// One sweep pass: walk the pool oldest-first, greedily collect mutually
/// compatible tickets within everyone's count bounds, form matches.
pub fn try_match() {
    let tickets = pool().read().expect("matchmaker pool lock").clone();
    if tickets.len() < 2 {
        return;
    }
    let mut used: HashSet<Uuid> = HashSet::new();
    let mut formed: Vec<Vec<Ticket>> = Vec::new();
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
            // Every pair in the group must satisfy each other's query.
            let compatible = group.iter().all(|g| {
                query_matches(&g.query, &candidate.string_props, &candidate.numeric_props)
                    && query_matches(&candidate.query, &g.string_props, &g.numeric_props)
            }) && (group.len() as i32 + 1) <= candidate.max_count;
            if compatible {
                group.push(candidate.clone());
                used.insert(candidate.id);
            }
        }
        if group.len() as i32 >= group.iter().map(|t| t.min_count).max().unwrap_or(99) {
            formed.push(group);
        } else {
            // Too few people: release the strays back into the pool.
            for t in &group {
                used.remove(&t.id);
            }
        }
    }
    if formed.is_empty() {
        return;
    }
    pool()
        .write()
        .expect("matchmaker pool lock")
        .retain(|t| !used.contains(&t.id));
    for group in formed {
        let tokens: Vec<String> = group
            .iter()
            .map(|_| Uuid::new_v4().simple().to_string())
            .collect();
        let match_id = match_handler::create_with_tokens(0, tokens.clone());
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
                presence: presence::presence_of(&sess),
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
