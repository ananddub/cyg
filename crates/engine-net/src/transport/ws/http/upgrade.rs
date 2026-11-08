use std::io::Write;

use mio::Token;
use std::sync::Arc;
use std::time::Instant;

use super::super::client::{WsClient, WsState};
use super::super::hooks::ServerCtx;
use super::super::rfc6455::{encode_ws_frame, handle_ws_handshake};
use super::request::Greeting;
use super::seat::build_join_reply;
use crate::transport::TransportEvent;

pub(super) fn complete_upgrade(
    client: &mut WsClient,
    req: &Greeting,
    peer: &str,
    token: Token,
    events_out: &mut Vec<TransportEvent>,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    let Some(resp) = handle_ws_handshake(&client.recv_buf) else {
        // Headers are complete but carry no usable Sec-WebSocket-Key. Answer
        // with a real status so the peer fails fast instead of hanging until
        // its own handshake timeout, and close the connection.
        let _ = client.stream.write_all(
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return true;
    };
    let _ = client.stream.write_all(&resp);
    client.recv_buf.clear();
    client.state = WsState::Connected;
    client.last_activity = Instant::now();

    if let Some(sid) = &req.url_session_id {
        client.session_id = Arc::from(sid.as_str());
    }
    client.room_id = req.url_room.clone();
    // One lookup, one entry: room, options and seat token all live together.
    // The entry may already exist from matchmake, so update in place rather
    // than replacing it — replacing would drop the seat token AND reset the
    // socket token to `Token(0)`, after which publish could not find the
    // client and the join snapshot never shipped.
    // The entry usually already exists: matchmake creates it before the socket
    // upgrade so the seat token is waiting. Always overwrite `token` with the
    // real registration — a meta left at `Token(0)` makes publish unable to
    // find this client, so the join snapshot never ships.
    let meta = ctx
        .clients_meta
        .entry(client.session_id.clone())
        .or_insert_with(|| super::super::hooks::ClientMeta::new(token));
    meta.token = token;
    let seat = meta.seat.clone();
    if !req.url_room.is_empty() {
        meta.room = Some(std::sync::Arc::from(req.url_room.as_str()));
    }
    if let Some(options) = &req.url_options {
        meta.options = Some(std::sync::Arc::from(options.as_str()));
    }

    let reply = build_join_reply(&client.session_id, &client.room_id, seat.as_deref());
    encode_ws_frame(&reply, &mut client.send_queue);

    if !ctx.schema_descriptor.is_empty() {
        encode_ws_frame(ctx.schema_descriptor, &mut client.send_queue);
    }

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
