//! Completing the RFC 6455 upgrade and starting the game protocol.

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use super::super::client::{WsClient, WsState};
use super::super::hooks::ServerCtx;
use super::super::rfc6455::{encode_ws_frame, handle_ws_handshake};
use super::request::Greeting;
use super::seat::build_join_reply;
use crate::transport::TransportEvent;

/// Finish the `101 Switching Protocols` handshake. Returns `false`: an upgrade
/// keeps the connection, and a buffer that holds no complete request yet is
/// simply left for the next readable event.
pub(super) fn complete_upgrade(
    client: &mut WsClient,
    req: &Greeting,
    peer: &str,
    events_out: &mut Vec<TransportEvent>,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    let Some(resp) = handle_ws_handshake(&client.recv_buf) else {
        return false;
    };
    let _ = client.stream.write_all(&resp);
    client.recv_buf.clear();
    client.state = WsState::Connected;
    client.last_activity = Instant::now();

    // Adopt the SDK's seat-reservation sessionId from the upgrade URL so the server-side player key matches room.sessionId on the client.
    if let Some(sid) = &req.url_session_id {
        client.session_id = Arc::from(sid.as_str());
    }
    client.room_id = req.url_room.clone();
    if !req.url_room.is_empty() {
        ctx.client_rooms
            .insert(client.session_id.to_string(), req.url_room.clone());
    }

    // In this protocol the SERVER initiates the join: the SDK client connects, then waits for the JOIN_ROOM handshake before `onJoin`.
    let reply = build_join_reply(&client.session_id, &client.room_id, ctx.seat_tokens);
    encode_ws_frame(&reply, &mut client.send_queue);

    if let Ok(peer_addr) = client.stream.peer_addr() {
        events_out.push(TransportEvent::Connected {
            client_id: client.session_id.clone(),
            peer_addr,
        });
    } else if let Some(addr) = peer.parse().ok() {
        events_out.push(TransportEvent::Connected {
            client_id: client.session_id.clone(),
            peer_addr: addr,
        });
    }
    false
}
