//! `party_data_send`: relay a payload to the rest of the squad.

use std::sync::Arc;
use uuid::Uuid;

use crate::error::V2Error;
use crate::infra::ws::envelope::{Envelope, EnvelopeMessage};
use crate::infra::ws::session;
use crate::services::presence;

use super::parties;

pub fn data_send(
    s: &Arc<session::Session>,
    raw_id: &str,
    op_code: i64,
    data: String,
    cid: Option<String>,
) -> Result<(), V2Error> {
    let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("party not found"))?;
    let sender = presence::presence_of(s);
    let event = Envelope {
        cid: None,
        message: EnvelopeMessage::PartyData {
            party_id: id.simple().to_string(),
            presence: Some(sender.clone()),
            op_code,
            data: data.clone(),
        },
    };
    {
        let all = parties().read().expect("party registry lock");
        let p = all
            .get(&id)
            .ok_or_else(|| V2Error::not_found("party not found"))?;
        if !p.members.contains_key(&s.sid) {
            return Err(V2Error::permission_denied(
                "must join the party before sending data",
            ));
        }
        for sid in p.members.keys() {
            if *sid != s.sid {
                if let Some(sess) = session::find(*sid) {
                    sess.send(&event);
                }
            }
        }
    }
    s.send_message(
        cid,
        EnvelopeMessage::PartyDataSend {
            party_id: id.simple().to_string(),
            op_code,
            data,
        },
    );
    Ok(())
}
