//! Party-scoped matchmaker tickets: the pool entry rides the caller's session,
//! and the matched notice reaches the whole squad through each member's socket.

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{EnvelopeMessage, ErrorMsg};
use crate::infra::ws::session;
use crate::services::matchmaker;

pub fn matchmaker_add(
    s: &Arc<session::Session>,
    min_count: i32,
    max_count: i32,
    query: String,
    string_props: HashMap<String, String>,
    numeric_props: HashMap<String, f64>,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let Some(party_id) = *s.party.read().expect("party lock") else {
        s.send_message(
            cid,
            EnvelopeMessage::Error(ErrorMsg {
                message: "not a member of a party".into(),
            }),
        );
        return Ok(());
    };
    let ticket = match matchmaker::add(s, min_count, max_count, query, string_props, numeric_props)
    {
        Ok(t) => t,
        Err(msg) => {
            s.send_message(cid, msg);
            return Ok(());
        }
    };
    s.send_message(
        cid,
        EnvelopeMessage::PartyMatchmakerTicket {
            party_id: party_id.simple().to_string(),
            ticket,
        },
    );
    Ok(())
}

pub fn matchmaker_remove(
    s: &Arc<session::Session>,
    raw_party: &str,
    ticket: &str,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let party_id = Uuid::parse_str(raw_party).map_err(|_| V2Error::not_found("party not found"))?;
    if *s.party.read().expect("party lock") != Some(party_id) {
        return Err(V2Error::permission_denied("not a member of this party"));
    }
    if matchmaker::remove(s, ticket) {
        s.send_message(cid, EnvelopeMessage::MatchmakerDone);
    }
    Ok(())
}
