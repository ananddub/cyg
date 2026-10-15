//! The one ordered event pass, plus the send surface.
//!
//! `impl GameTransport for WsTransport` cannot be split across files, so
//! `poll_events` keeps its single `for event in events.iter()` pass here and
//! delegates each arm to `accept::on_listener_event` /
//! `client_io::on_client_event`. Extracting the arms must not turn the pass
//! into two: the interleave of heartbeat kills, accepts and disconnects is
//! observable in the event stream.

use std::io::{self, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut};

use super::accept::on_listener_event;
use super::client::WsState;
use super::client_io::on_client_event;
use super::hooks::Pass;
use super::protocol::wire_opcode_for;
use super::rfc6455::{encode_ws_frame, finish_close};
use super::transport::WsTransport;
use crate::framing::OpCode;
use crate::transport::{GameTransport, TransportEvent};

impl GameTransport for WsTransport {
    fn poll_events(&mut self, timeout: Duration, ev: &mut Vec<TransportEvent>) -> io::Result<()> {
        self.poll.poll(&mut self.events, Some(timeout))?;

        let now = Instant::now();

        // Adopt the persistent cross-poll buffer, seeded with events queued outside poll_events (e.g. `disconnect()`).
        let mut buf = std::mem::take(&mut self.ev);
        std::mem::swap(&mut buf, &mut self.pending_events);
        self.pending_events.clear();

        // Heartbeat: terminate clients that stopped answering pings.
        self.run_heartbeat(now);
        let mut hb_kills = std::mem::take(&mut self.hb_kills);
        for token in hb_kills.iter() {
            if let Some(mut client) = self.clients.remove(token) {
                let cid = client.session_id.clone();
                let _ = self.poll.registry().deregister(&mut client.stream);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        hb_kills.clear();
        self.hb_kills = hb_kills;

        // The arms need field borrows, so the maps are adopted for the duration of each pass and restored afterwards (no fresh Vecs).
        let mut clients = std::mem::take(&mut self.clients);
        let mut disconnected = std::mem::take(&mut self.disconnected);
        {
            let events = &self.events;
            let mut pass = Pass {
                poll: &mut self.poll,
                listeners: &mut self.listeners,
                clients: &mut clients,
                client_tokens: &mut self.client_tokens,
                client_rooms: &mut self.client_rooms,
                retained: &self.retained,
                seat_tokens: &mut self.seat_tokens,
                matchmake_hook: &mut self.matchmake_hook,
                http_hook: &mut self.http_hook,
                next_token: &mut self.next_token,
                disconnected: &mut disconnected,
                buf: &mut buf,
            };
            for event in events.iter() {
                let tok = event.token();
                if tok.0 < pass.listeners.len() {
                    on_listener_event(&mut pass, tok.0, now)?;
                } else {
                    on_client_event(&mut pass, event, now);
                }
            }
        }

        for token in disconnected.drain(..) {
            if let Some(mut client) = clients.remove(&token) {
                let cid = client.session_id.clone();
                // Deliver any queued close frame (e.g. LEAVE_ROOM's `4000 CONSENTED`) before the socket dies.
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
                self.client_tokens.remove(&*cid);
                self.client_rooms.remove(&*cid);
                self.seat_tokens.remove(&*cid);
                buf.push(TransportEvent::Disconnected { client_id: cid });
            }
        }
        self.clients = clients;
        self.disconnected = disconnected;

        std::mem::swap(ev, &mut buf);
        buf.clear();
        self.ev = buf;
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        self.queue_to(client_id, wire_opcode_for(opcode), payload);
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let colyseus_opcode = wire_opcode_for(opcode);
        let (sb, clients) = (&mut self.sb, &mut self.clients);
        sb.reserve(11 + payload.len());
        sb.put_u8(colyseus_opcode);
        sb.put_slice(payload);
        // Freeze first so the encoder can read the message and write the framed bytes into the same scratch in two disjoint steps.
        let msg: bytes::Bytes = sb.split().freeze();
        encode_ws_frame(&msg, sb);
        let frame: bytes::Bytes = sb.split().freeze();
        for client in clients.values_mut() {
            if matches!(client.state, WsState::Connected) {
                client.send_queue.extend_from_slice(&frame);
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for client in self.clients.values_mut() {
            if !client.send_queue.is_empty() {
                match client.stream.write(&client.send_queue) {
                    Ok(n) => {
                        client.send_queue.advance(n);
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.clients
            .values()
            .filter(|c| matches!(c.state, WsState::Connected))
            .count()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bound_addr)
    }

    fn transport_name(&self) -> &'static str {
        "WireSocket (SDK Compatible)"
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        WsTransport::disconnect(self, client_id)
    }
}
