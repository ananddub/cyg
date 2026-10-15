//! `matchmaker_add` / `matchmaker_remove`.

use auto_ws::{Conn, Msg};

use crate::error::V2Error;
use crate::infra::ws::envelope::{EnvelopeMessage, MatchmakerAdd};
use crate::services::matchmaker;

use super::super::messages::*;
use super::super::{Dispatched, session_of, unwrap_env_err};

pub(crate) async fn add(conn: &Conn, msg: &Msg) -> Dispatched {
    let add: MatchmakerAdd = msg.payload()?;
    matchmaker::add(
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

pub(crate) async fn remove(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchmakerRemove = msg.payload()?;
    if matchmaker::remove(&session_of(conn), &req.ticket) {
        Ok(Some(EnvelopeMessage::MatchmakerDone))
    } else {
        Err(V2Error::not_found("matchmaker ticket not found"))
    }
}
