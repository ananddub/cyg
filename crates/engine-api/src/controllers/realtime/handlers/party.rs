//! `party_*` events.

use auto_ws::{Conn, Msg};

use crate::services::party;

use super::super::messages::*;
use super::super::{Dispatched, session_of};

pub(crate) async fn create(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyCreate = msg.payload()?;
    party::create(
        &session_of(conn),
        req.max_size,
        req.open,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn join(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyJoin = msg.payload()?;
    party::join(&session_of(conn), &req.party_id, msg.cid().map(str::to_string))
        .map(|_| None)
}

pub(crate) async fn leave(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyLeave = msg.payload()?;
    party::leave(&session_of(conn), &req.party_id, msg.cid().map(str::to_string))
        .map(|_| None)
}

pub(crate) async fn promote(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyPromote = msg.payload()?;
    party::promote(
        &session_of(conn),
        &req.party_id,
        req.presence,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn update(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyUpdate = msg.payload()?;
    party::update(
        &session_of(conn),
        &req.party_id,
        req.max_size,
        req.open,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn close(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyClose = msg.payload()?;
    party::close(
        &session_of(conn),
        &req.party_id,
        req.metadata,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn data_send(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyDataSend = msg.payload()?;
    party::data_send(
        &session_of(conn),
        &req.party_id,
        req.op_code,
        req.data,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn matchmaker_add(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyMatchmakerAdd = msg.payload()?;
    party::matchmaker_add(
        &session_of(conn),
        req.min_count,
        req.max_count,
        req.query,
        req.string_properties,
        req.numeric_properties,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}

pub(crate) async fn matchmaker_remove(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: PartyMatchmakerRemove = msg.payload()?;
    party::matchmaker_remove(
        &session_of(conn),
        &req.party_id,
        &req.ticket,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}
