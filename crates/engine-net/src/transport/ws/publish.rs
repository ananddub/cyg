//! Server-initiated messages: the `ERROR` and `ROOM_RESPONSE` frames and the
//! immediate disconnect.

use std::io;
use std::sync::Arc;
use std::time::Instant;

use bytes::BufMut;

use super::client::MAX_SEND_QUEUE;
use super::msgpack::{mp_number, mp_str};
use super::protocol::{PROTO_ERROR, PROTO_ROOM_RESPONSE, close_code};
use super::rfc6455::{encode_wire_frame, finish_close, queue_close_frame};
use super::transport::WsTransport;
use crate::transport::TransportEvent;

impl WsTransport {
    /// Send a Colyseus `ERROR` (11) frame: surfaces to the SDK's
    /// `room.onError` handler. Wire: `[11][msgpack code][msgpack string]`.
    pub fn send_error(&mut self, client_id: &str, code: u16, message: &str) -> io::Result<()> {
        let mut payload = std::mem::take(&mut self.sb);
        payload.reserve(8 + message.len());
        mp_number(&mut payload, code as f64);
        mp_str(&mut payload, message);
        self.queue_to(client_id, PROTO_ERROR, &payload);
        payload.clear();
        self.sb = payload;
        Ok(())
    }

    /// Reply to a `ROOM_REQUEST` (21). Wire:
    /// `[22][msgpack requestId][status byte][msgpack payload]`.
    pub fn send_room_response(
        &mut self,
        client_id: &str,
        request_id: u32,
        status: u8,
        payload_msgpack: &[u8],
    ) -> io::Result<()> {
        let mut payload = std::mem::take(&mut self.sb);
        payload.reserve(8 + payload_msgpack.len());
        mp_number(&mut payload, request_id as f64);
        payload.put_u8(status);
        payload.extend_from_slice(payload_msgpack);
        self.queue_to(client_id, PROTO_ROOM_RESPONSE, &payload);
        payload.clear();
        self.sb = payload;
        Ok(())
    }

    /// Queue one Colyseus message (opcode byte + payload) as a WS frame for
    /// the given session.
    #[inline]
    pub(super) fn queue_to(&mut self, client_id: &str, colyseus_opcode: u8, payload: &[u8]) {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(client) = self.clients.get_mut(&token) {
                let required = payload.len() + 16;
                if client.send_queue.len() + required > MAX_SEND_QUEUE {
                    // Peer stopped reading: drop the frame instead of growing
                    // the queue forever. The socket stays open so a transient
                    // stall can recover.
                    self.dropped_frames = self.dropped_frames.saturating_add(1);
                    if self.last_drop_log.elapsed().as_secs() >= 1 {
                        eprintln!(
                            "[ws] peer {client_id} not draining ({} KB queued) — {} frames dropped",
                            client.send_queue.len() / 1024,
                            self.dropped_frames
                        );
                        self.last_drop_log = Instant::now();
                    }
                    return;
                }
                client.send_queue.reserve(required);
                encode_wire_frame(colyseus_opcode, payload, &mut client.send_queue);
                self.flush_dirty = true;
            }
        }
    }

    /// Queue the same wire message for several sessions with one encode.
    /// Server frames are unmasked, so every recipient gets identical bytes.
    #[inline]
    pub(super) fn send_many(&mut self, client_ids: &[&str], colyseus_opcode: u8, payload: &[u8]) {
        self.sb.clear();
        self.sb.reserve(payload.len() + 11);
        encode_wire_frame(colyseus_opcode, payload, &mut self.sb);
        let frame = &self.sb[..];
        let frame_len = frame.len();
        let mut queued = false;
        for &client_id in client_ids {
            let Some(&token) = self.client_tokens.get(client_id) else {
                continue;
            };
            let Some(client) = self.clients.get_mut(&token) else {
                continue;
            };
            let cur_len = client.send_queue.len();
            if cur_len + frame_len > MAX_SEND_QUEUE {
                self.dropped_frames = self.dropped_frames.saturating_add(1);
                continue;
            }
            if client.send_queue.capacity().saturating_sub(cur_len) >= frame_len {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        frame.as_ptr(),
                        client.send_queue.as_mut_ptr().add(cur_len),
                        frame_len,
                    );
                    client.send_queue.set_len(cur_len + frame_len);
                }
            } else {
                client.send_queue.extend_from_slice(frame);
            }
            queued = true;
        }
        if queued {
            self.flush_dirty = true;
        }
    }

    /// Drop a client immediately: sends the WS close frame (`4000 CONSENTED`)
    /// and surfaces a `Disconnected` event on the next `poll_events`.
    pub fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        self.disconnect_with(client_id, close_code::CONSENTED, "server disconnect")
    }

    /// Same as [`Self::disconnect`] but with a caller-chosen close code/reason
    /// (e.g. `close_code::WITH_ERROR` + "room is full").
    pub fn disconnect_with(&mut self, client_id: &str, code: u16, reason: &str) -> io::Result<()> {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(mut client) = self.clients.remove(&token) {
                queue_close_frame(&mut client, code, reason);
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
            }
            self.client_tokens.remove(client_id);
            self.client_rooms.remove(client_id);
            self.seat_tokens.remove(client_id);
            self.pending_events.push(TransportEvent::Disconnected {
                client_id: Arc::from(client_id),
            });
        }
        Ok(())
    }
}
