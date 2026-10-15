//! `status_follow` / `status_unfollow` / `status_update`.

use auto_ws::{Conn, Msg};

use crate::services::presence;

use super::super::messages::*;
use super::super::{Dispatched, state_of};

pub(crate) async fn follow(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: StatusFollow = msg.payload()?;
    presence::follow(
        &state_of(conn),
        conn.id(),
        req.user_ids,
        req.usernames,
        msg.cid().map(str::to_string),
    )
    .await
    .map(|_| None)
}

pub(crate) async fn unfollow(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: StatusUnfollow = msg.payload()?;
    presence::unfollow(conn.id(), req.user_ids, msg.cid().map(str::to_string));
    Ok(None)
}

pub(crate) async fn update(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: StatusUpdate = msg.payload()?;
    presence::update_status(&state_of(conn), conn.id(), req.status, msg.cid().map(str::to_string));
    Ok(None)
}
