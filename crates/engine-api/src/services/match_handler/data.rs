//! `match_data_send` relay and `match_signal` one-shot presence ping.

use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;

use super::{matches, notify, presence_of};

/// `match_data_send`: relay to every other member, sender presence attached.
pub fn data_send(
    s: &Arc<session::Session>,
    raw_id: &str,
    op_code: i64,
    data: String,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
    let (recipients, sender) = {
        let all = matches().read().expect("match registry lock");
        let m = all
            .get(&id)
            .ok_or_else(|| V2Error::not_found("match not found"))?;
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
        (recipients, presence_of(s))
    };
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
    notify(&recipients, s.sid, &event);
    Ok(())
}

/// `match_signal`: one-shot presence ping (the MatchSignal), relayed
/// to everyone in the match except the sender.
pub fn signal(
    s: &Arc<session::Session>,
    raw_id: &str,
    data: String,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
    let member_uids: Vec<Uuid> = {
        let all = matches().read().expect("match registry lock");
        let m = all
            .get(&id)
            .ok_or_else(|| V2Error::not_found("match not found"))?;
        m.members
            .values()
            .map(|p| Uuid::parse_str(&p.user_id).unwrap_or_default())
            .collect()
    };
    let event = Envelope {
        cid: None,
        message: EnvelopeMessage::MatchSignal {
            match_id: id.simple().to_string(),
            data,
        },
    };
    notify(&member_uids, s.sid, &event);
    s.send_message(
        cid,
        EnvelopeMessage::MatchSignal {
            match_id: id.simple().to_string(),
            data: String::new(),
        },
    );
    Ok(())
}
