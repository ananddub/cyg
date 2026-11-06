
use std::io::{self, Write};
use std::time::Instant;

use super::client::WsClient;
use bytes::Buf;
use mio::event::Event;
use mio::{Interest, Token};

use super::client::WsState;
use super::frames::process_ws_frames;
use super::hooks::{Pass, ServerCtx};
use super::http::handle_client_greeting;

pub(super) fn on_client_event(pass: &mut Pass<'_>, event: &Event, now: Instant) {
    let token = event.token();
    let mut conn_dead = false;

    if let Some(client) = pass.clients.get_mut(&token) {
        if event.is_readable() {
            loop {
                match crate::bufio::read_tcp(&mut client.stream, &mut client.recv_buf, 4096) {
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
                            seat_tokens: &mut *pass.seat_tokens,
                            client_rooms: &mut *pass.client_rooms,
                            client_options: &mut *pass.client_options,
                            game_port: pass.game_port,
                            udp_port: pass.udp_port,
                            schema_descriptor: pass.schema_descriptor,
                            allowed_origins: pass.allowed_origins,
                        };
                        handle_client_greeting(client, &peer, &mut *pass.buf, &mut ctx)
                    };
                    if closed {
                        conn_dead = true;
                    } else {

                        if client.session_id != prev_id {
                            pass.client_tokens.remove(&*prev_id);
                            pass.client_tokens
                                .insert(client.session_id.clone(), token);
                        }
                        if client.state == WsState::Connected {

                            conn_dead |= process_ws_frames(client, &mut *pass.buf, now, pass.simulated_latency);
                        }
                    }
                }
                WsState::Connected => {

                    conn_dead |= process_ws_frames(client, &mut *pass.buf, now, pass.simulated_latency);
                }
            }
        }

        shrink_read_buffers(client);

        if event.is_writable() && !client.send_queue.is_empty() {
            match client.stream.write(&client.send_queue) {
                Ok(n) => {
                    client.send_queue.advance(n);
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
        client.recv_buf = bytes::BytesMut::with_capacity(4096);
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
