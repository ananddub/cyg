use std::io;
use std::time::Instant;

use bytes::BufMut;

use super::client::{MAX_OVERFLOW_STRIKES, MAX_SEND_QUEUE};
use super::msgpack::{mp_number, mp_str};
use super::protocol::close_code;
use super::protocol::{PROTO_ERROR, PROTO_ROOM_RESPONSE};
use super::rfc6455::encode_wire_frame;
use super::transport::WsTransport;

impl WsTransport {
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

    /// Queue one shared delta frame: head (magic + your_id) inline, body as
    /// the shared Arc, tail (acked_seq + server_tick) after it. Falls back to
    /// a plain inline copy when the client already has buffered inline bytes
    /// (rare) so ordering is never broken.
    pub fn queue_shared_delta(
        &mut self,
        client_id: &str,
        head: &[u8],
        body: std::sync::Arc<[u8]>,
        tail: &[u8; 8],
    ) -> bool {
        let Some(token) = self.clients_meta.get(client_id).map(|m| m.token) else {
            return false;
        };
        let Some(client) = self.clients.get_mut(&token) else {
            return false;
        };
        if !matches!(client.state, super::client::WsState::Connected) {
            return false;
        }
        let body_len = body.len();
        let total = head.len() + body_len + 6;
        if client.send_queue.is_empty()
            && client.shared_frame.is_none()
            && client.post_shared.is_empty()
        {
            if body_len == 0 {
                return false;
            }
            if total > MAX_SEND_QUEUE {
                self.dropped_frames = self.dropped_frames.saturating_add(1);
                client.overflow_strikes += 1;
                return false;
            }
            client.send_queue.extend_from_slice(head);
            client.shared_frame = Some((body, 0));
            client.shared_pending = true;
            client.post_shared.extend_from_slice(tail); // pump writes: head -> body -> tail
            self.flush_dirty = true;
            return true;
        }
        // Shared path must keep the SAME slow-consumer contract as queue_to /
        // send_many: strike on overflow, disconnect at MAX_OVERFLOW_STRIKES.
        // Without the kick a stalled client sat forever at the cap holding
        // its buffers + the body Arc (the shared-queue RSS leak).
        let mut kick = false;
        {
            // Scoped block: ends the clients-map borrow explicitly instead of
            // a no-op `drop(client)` on a `&mut` reference.
            if client.send_queue.len() + client.post_shared.len() + total > MAX_SEND_QUEUE {
                self.dropped_frames = self.dropped_frames.saturating_add(1);
                client.overflow_strikes += 1;
                kick = client.overflow_strikes >= MAX_OVERFLOW_STRIKES;
                if kick || self.last_drop_log.elapsed().as_secs() >= 1 {
                    tracing::info!(
                        "[ws] peer {client_id} not draining shared deltas ({} KB queued, {}/{} strikes){}",
                        client.send_queue.len() / 1024,
                        client.overflow_strikes,
                        MAX_OVERFLOW_STRIKES,
                        if kick { " — disconnecting" } else { "" }
                    );
                    self.last_drop_log = Instant::now();
                }
                if !kick {
                    return false;
                }
            } else {
                client.send_queue.extend_from_slice(head);
                client.send_queue.extend_from_slice(&body);
                client.send_queue.extend_from_slice(tail);
                self.flush_dirty = true;
                return true;
            }
        }
        let _ = self.disconnect_with(
            client_id,
            close_code::SLOW_CONSUMER,
            "send queue overflow (slow consumer)",
        );
        false
    }

    #[inline]
    pub(super) fn queue_to(&mut self, client_id: &str, colyseus_opcode: u8, payload: &[u8]) {
        if let Some(token) = self.clients_meta.get(client_id).map(|m| m.token) {
            if let Some(client) = self.clients.get_mut(&token) {
                client.state_frame_start = None;
                let required = payload.len() + 16;
                if client.send_queue.len() + required > MAX_SEND_QUEUE {
                    self.dropped_frames = self.dropped_frames.saturating_add(1);
                    client.overflow_strikes += 1;
                    let kick = client.overflow_strikes >= MAX_OVERFLOW_STRIKES;
                    if kick || self.last_drop_log.elapsed().as_secs() >= 1 {
                        tracing::info!(
                            "[ws] peer {client_id} not draining ({} KB queued, {}/{} strikes){}",
                            client.send_queue.len() / 1024,
                            client.overflow_strikes,
                            MAX_OVERFLOW_STRIKES,
                            if kick { " — disconnecting" } else { "" }
                        );
                        self.last_drop_log = Instant::now();
                    }
                    if kick {
                        let _ = self.disconnect_with(
                            client_id,
                            close_code::SLOW_CONSUMER,
                            "send queue overflow (slow consumer)",
                        );
                        return;
                    }
                }
                client.send_queue.reserve(required);
                encode_wire_frame(colyseus_opcode, payload, &mut client.send_queue);
                self.flush_dirty = true;
            }
        }
    }

    #[inline]
    pub(super) fn send_many(&mut self, client_ids: &[&str], colyseus_opcode: u8, payload: &[u8]) {
        self.sb.clear();
        self.sb.reserve(payload.len() + 11);
        encode_wire_frame(colyseus_opcode, payload, &mut self.sb);
        let frame = &self.sb[..];
        let frame_len = frame.len();
        let mut queued = false;
        let mut kicked: Vec<&str> = Vec::new();
        for &client_id in client_ids {
            let Some(token) = self.clients_meta.get(client_id).map(|m| m.token) else {
                continue;
            };
            let Some(client) = self.clients.get_mut(&token) else {
                continue;
            };
            client.state_frame_start = None;
            let cur_len = client.send_queue.len();
            if cur_len + frame_len > MAX_SEND_QUEUE {
                self.dropped_frames = self.dropped_frames.saturating_add(1);
                client.overflow_strikes += 1;
                if client.overflow_strikes >= MAX_OVERFLOW_STRIKES {
                    kicked.push(client_id);
                    continue;
                }
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
        for client_id in kicked {
            let _ = self.disconnect_with(
                client_id,
                close_code::SLOW_CONSUMER,
                "send queue overflow (slow consumer)",
            );
        }
        if queued {
            self.flush_dirty = true;
        }
    }
}
