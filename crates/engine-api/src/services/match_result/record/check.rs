use std::collections::HashMap;

use uuid::Uuid;

use crate::dto::match_result::{MatchResultRequest, ParticipantResult};
use crate::error::V2Error;
use crate::repositories::match_result::CounterWrite;
use crate::services::match_result::{MAX_COUNTERS, MAX_INCREMENT, MAX_PARTICIPANTS, MAX_SCORE};
use crate::services::quest::catalog::counter_name;
use crate::services::realtime::RealtimeMatchService;
use crate::utils::leaderboard::types::BAD_METADATA;

/// The players the control plane seated in this match, with the name each one
/// held on the socket, which is what a leaderboard record shows.
pub(super) fn seated(match_id: Uuid) -> Option<HashMap<Uuid, String>> {
    let snapshot = RealtimeMatchService::get_snapshot(match_id)?;
    let mut seats = HashMap::with_capacity(snapshot.members.len());
    for member in snapshot.members {
        if let Ok(uid) = Uuid::parse_str(&member.user_id) {
            seats.insert(uid, member.username);
        }
    }
    Some(seats)
}

/// Every named player has to be one that was seated, and seated once.
pub(super) fn check(
    req: &MatchResultRequest,
    seats: &HashMap<Uuid, String>,
) -> Result<(), V2Error> {
    if req.participants.is_empty() || req.participants.len() > MAX_PARTICIPANTS {
        return Err(V2Error::invalid_argument(format!(
            "A match result names between 1 and {MAX_PARTICIPANTS} players."
        )));
    }
    let mut named: Vec<Uuid> = Vec::with_capacity(req.participants.len());
    for part in &req.participants {
        if named.contains(&part.user_id) {
            return Err(V2Error::invalid_argument(
                "A match result names each player once.",
            ));
        }
        if !seats.contains_key(&part.user_id) {
            return Err(V2Error::invalid_argument(
                "A match result can only name a player who was in the match.",
            ));
        }
        if !(part.score.abs() <= MAX_SCORE && part.subscore.abs() <= MAX_SCORE) {
            return Err(V2Error::invalid_argument(format!(
                "A match score is between -{MAX_SCORE} and {MAX_SCORE}."
            )));
        }
        if metadata_rejected(part) {
            return Err(V2Error::invalid_argument(BAD_METADATA));
        }
        named.push(part.user_id);
    }
    Ok(())
}

fn metadata_rejected(part: &ParticipantResult) -> bool {
    part.metadata
        .as_ref()
        .is_some_and(|value| !value.is_object())
}

/// The counters this result raises, each named the way an event names one. A
/// result can only raise a counter, never set or lower it: the host reports what
/// happened, and history is what the counters already hold.
pub(super) fn counter_writes(req: &MatchResultRequest) -> Result<Vec<CounterWrite>, V2Error> {
    let mut writes = Vec::new();
    for part in &req.participants {
        for (name, delta) in &part.stats {
            let stat = counter_name(name)?.to_string();
            if !(1..=MAX_INCREMENT).contains(delta) {
                return Err(V2Error::invalid_argument(format!(
                    "The {stat} increment must be between 1 and {MAX_INCREMENT}."
                )));
            }
            writes.push(CounterWrite {
                user_id: part.user_id,
                stat,
                delta: *delta,
            });
        }
    }
    if writes.len() > MAX_COUNTERS {
        return Err(V2Error::invalid_argument(format!(
            "A match result raises at most {MAX_COUNTERS} counters."
        )));
    }
    Ok(writes)
}
