//! The listener arm of the poll: accept, read the first bytes, build the
//! client and pump its greeting.

use std::io;
use std::time::Instant;

use bytes::BytesMut;
use mio::{Interest, Token};

use super::client::{WsClient, WsState};
use super::frames::process_ws_frames;
use super::hooks::{Pass, ServerCtx};
use super::http::handle_client_greeting;

pub(super) fn on_listener_event(
    pass: &mut Pass<'_>,
    listener_idx: usize,
    now: Instant,
) -> io::Result<()> {
    loop {
        match pass.listeners[listener_idx].accept() {
            Ok((mut stream, peer)) => {
                let token = Token(*pass.next_token);
                *pass.next_token += 1;
                stream.set_nodelay(true)?;

                // READABLE only: WRITABLE is added on send backpressure
                // (level-triggered epoll would spin on an always-writable
                // socket, one busy event per client per poll).
                pass.poll
                    .registry()
                    .register(&mut stream, token, Interest::READABLE)?;

                // Read the client's first bytes (HTTP request) straight into the BytesMut — no stack copy.
                let mut first_bytes: BytesMut = BytesMut::new();
                loop {
                    match crate::bufio::read_tcp(&mut stream, &mut first_bytes, 4096) {
                        Ok(0) => break,
                        Ok(_) => continue,
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => break,
                    }
                }

                // Session id is finalized inside the greeting (the SDK appends its seat-reservation sessionId to the upgrade URL). Track token -> id for send_to().
                let has_first_bytes = !first_bytes.is_empty();
                let mut client = WsClient {
                    stream,
                    state: WsState::Handshaking,
                    session_id: format!("colyseus_{}_{}", token.0, peer.port()).into(),
                    room_id: String::new(), // filled by greeting
                    recv_buf: first_bytes,
                    send_queue: BytesMut::with_capacity(4096),
                    frag_opcode: None,
                    frag_buf: BytesMut::new(),
                    last_activity: now,
                    ping_sent_at: None,
                    missed_pings: 0,
                    consented: false,
                    state_frame_start: None,
                    state_frame_len: 0,
                    registered_writable: false,
                };

                if has_first_bytes {
                    let close = {
                        let mut ctx = ServerCtx {
                            matchmake_hook: pass.matchmake_hook.as_ref(),
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
                        handle_client_greeting(
                            &mut client,
                            &peer.to_string(),
                            &mut *pass.buf,
                            &mut ctx,
                        )
                    };
                    if close {
                        // HTTP request served; nothing to track.
                        continue;
                    }
                    // Frames may have arrived together with the upgrade request (edge-triggered epoll).
                    if client.state == WsState::Connected && !client.recv_buf.is_empty() {
                        process_ws_frames(&mut client, &mut *pass.buf, now);
                    }
                }
                // Real WebSocket client: keep it registered (data may not have arrived yet — greeting re-runs on the readable event).
                pass.client_tokens
                    .insert(client.session_id.to_string(), token);
                pass.clients.insert(token, client);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
