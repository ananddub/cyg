//! `channel_*` and `stream_data`.

use auto_ws::{Conn, Msg};

use crate::services::chat;

use super::super::messages::*;
use super::super::{Dispatched, session_of, state_of};

pub(crate) async fn join(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: ChannelJoin = msg.payload()?;
    chat::join(
        &state_of(conn),
        &session_of(conn),
        &req.channel_id,
        req.persistent,
        msg.cid().map(str::to_string),
    )
    .await
    .map(|_| None)
}

pub(crate) async fn leave(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: ChannelLeave = msg.payload()?;
    chat::leave(&session_of(conn), &req.channel_id, msg.cid().map(str::to_string))
        .map(|_| None)
}

pub(crate) async fn message_send(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: ChannelMessageSend = msg.payload()?;
    chat::message_send(
        &state_of(conn),
        &session_of(conn),
        &req.channel_id,
        req.content,
        req.persist,
        req.ephemeral,
        msg.cid().map(str::to_string),
    )
    .await
    .map(|_| None)
}

pub(crate) async fn message_update(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: ChannelMessageUpdate = msg.payload()?;
    chat::message_update(
        &state_of(conn),
        &session_of(conn),
        &req.channel_id,
        &req.message_id,
        req.content,
        msg.cid().map(str::to_string),
    )
    .await
    .map(|_| None)
}

pub(crate) async fn message_remove(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: ChannelMessageRemove = msg.payload()?;
    chat::message_remove(
        &state_of(conn),
        &session_of(conn),
        &req.channel_id,
        &req.message_id,
        msg.cid().map(str::to_string),
    )
    .await
    .map(|_| None)
}

pub(crate) async fn stream_data(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: StreamData = msg.payload()?;
    chat::stream_data(
        &session_of(conn),
        &req.stream_id,
        req.data,
        req.reliable,
        msg.cid().map(str::to_string),
    )
    .map(|_| None)
}
