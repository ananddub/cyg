use std::io::{self, Write};
use std::time::Instant;

use super::client::WsClient;
use bytes::Buf;
use mio::event::Event;
use mio::{Interest, Token};

use super::client::WsState;
use super::frames::process_ws_frames;
use super::hooks::{ClientMeta, Pass, ServerCtx};
use super::http::handle_client_greeting;

pub(super) fn on_client_event(pass: &mut Pass<'_>, event: &Event, now: Instant) {
    let token = event.token();
    let mut conn_dead = false;

    if let Some(client) = pass.clients.get_mut(&token) {
        if event.is_readable() {
            loop {
                match crate::bufio::read_tcp(&mut client.stream, &mut client.recv_buf, 2048) {
                    Ok(0) => {
                        conn_dead = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        conn_dead = true;
                        break;
                    }
                }
            }

            match client.state {
                WsState::Handshaking => {
                    let prev_id = client.session_id.clone();
                    let peer = client
                        .stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_default();
                    let closed = {
                        let mut ctx = ServerCtx {
                            matchmake_hook: pass.matchmake_hook.as_ref(),
                            room_variants: pass.room_variants,
                            http_hook: pass.http_hook.as_ref(),
                            retained: pass.retained,
                            clients_meta: &mut *pass.clients_meta,
                            game_port: pass.game_port,
                            udp_port: pass.udp_port,
                            schema_descriptor: pass.schema_descriptor,
                            allowed_origins: pass.allowed_origins,
                        };
                        handle_client_greeting(client, &peer, token, &mut *pass.buf, &mut ctx)
                    };
                    if closed {
                        conn_dead = true;
                    } else {
                        if client.session_id != prev_id {
                            // The upgrade already created the entry under the
                            // real session id (with room + options). Prefer it:
                            // a `new()` default here would wipe the room and
                            // `ws_knows()` would then route state to WebRTC
                            // instead of the socket, so the join snapshot never
                            // reaches the client.
                            if !client.session_id.is_empty() {
                                let had_entry = pass.clients_meta.contains_key(&client.session_id);
                                if had_entry {
                                    // The upgrade already created the entry
                                    // under the real session id (with room and
                                    // options). Keep it and only refresh the
                                    // token — reinserting a default would wipe
                                    // the room and `ws_knows()` would route the
                                    // join snapshot to WebRTC instead of the
                                    // socket.
                                    if let Some(meta) =
                                        pass.clients_meta.get_mut(&client.session_id)
                                    {
                                        meta.token = token;
                                    }
                                    pass.clients_meta.remove(&*prev_id);
                                } else {
                                    // Carry the metadata across the rename so
                                    // room/options/seat survive a re-key.
                                    let mut meta = pass
                                        .clients_meta
                                        .remove(&*prev_id)
                                        .unwrap_or_else(|| ClientMeta::new(token));
                                    meta.token = token;
                                    pass.clients_meta.insert(client.session_id.clone(), meta);
                                }
                            }
                        }
                        if client.state == WsState::Connected {
                            conn_dead |= process_ws_frames(
                                client,
                                &mut *pass.buf,
                                now,
                                pass.simulated_latency,
                            );
                        }
                    }
                }
                WsState::Connected => {
                    conn_dead |=
                        process_ws_frames(client, &mut *pass.buf, now, pass.simulated_latency);
                }
            }
        }

        shrink_read_buffers(client);

        if event.is_writable() && (!client.send_queue.is_empty() || client.shared_frame.is_some()) {
            match client.write_out() {
                Ok(_) => {
                    client.state_frame_start = None;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => conn_dead = true,
            }
        }

        sync_writable_interest(client, pass.poll, event.token());
    }

    if conn_dead {
        pass.disconnected.push(token);
    }
}

/// Reclaims buffer capacity a burst (or a large inbound frame) left behind:
/// inputs are ~20B frames, so a drained reader never needs more than the
/// initial allocation. Keeps per-connection RSS at the floor under churn.
fn shrink_read_buffers(client: &mut WsClient) {
    if client.recv_buf.is_empty() && client.recv_buf.capacity() > 16 * 1024 {
        client.recv_buf = bytes::BytesMut::with_capacity(2048);
    }
    if client.frag_buf.is_empty() && client.frag_buf.capacity() > 16 * 1024 {
        client.frag_buf = bytes::BytesMut::new();
    }
    if client.post_shared.is_empty() && client.post_shared.capacity() > 4096 {
        client.post_shared = bytes::BytesMut::new();
    }
}

pub(super) fn sync_writable_interest(client: &mut WsClient, poll: &mio::Poll, token: Token) {
    let want = !client.send_queue.is_empty();
    if client.registered_writable == want {
        return;
    }
    let interest = if want {
        Interest::READABLE | Interest::WRITABLE
    } else {
        Interest::READABLE
    };
    if poll
        .registry()
        .reregister(&mut client.stream, token, interest)
        .is_ok()
    {
        client.registered_writable = want;
    }
}
