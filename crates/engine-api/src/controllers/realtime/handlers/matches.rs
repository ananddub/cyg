//! `match_create` / `match_join` / `match_leave` / `match_data_send` / `match_signal`.

use auto_ws::{Conn, Msg};

use crate::infra::ws::envelope::EnvelopeMessage;
use crate::services::match_handler;

use super::super::messages::*;
use super::super::{Dispatched, session_of};

pub(crate) async fn create(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchCreate = msg.payload()?;
    let id = match_handler::create(
        &session_of(conn),
        req.open,
        req.tick_rate,
        req.name.unwrap_or(req.label),
    );
    Ok(Some(EnvelopeMessage::MatchCreateAck { match_id: id.simple().to_string() }))
}

pub(crate) async fn join(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchJoin = msg.payload()?;
    match_handler::join(
        &session_of(conn),
        req.match_id,
        req.token,
        req.label,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn leave(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchLeave = msg.payload()?;
    match_handler::leave(&session_of(conn), &req.match_id, msg.cid().map(str::to_string))
        .map(|_| None)
}

pub(crate) async fn data_send(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchDataSend = msg.payload()?;
    match_handler::data_send(&session_of(conn), &req.match_id, req.op_code, req.data)
        .map(|_| None)
}

pub(crate) async fn signal(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: MatchSignal = msg.payload()?;
    match_handler::signal(
        &session_of(conn),
        &req.match_id,
        req.data,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}
