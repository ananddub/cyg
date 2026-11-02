
use std::io;
use std::time::Instant;

use bytes::BufMut;

use super::client::MAX_SEND_QUEUE;
use super::msgpack::{mp_number, mp_str};
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

    #[inline]
    pub(super) fn queue_to(&mut self, client_id: &str, colyseus_opcode: u8, payload: &[u8]) {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(client) = self.clients.get_mut(&token) {

                client.state_frame_start = None;
                let required = payload.len() + 16;
                if client.send_queue.len() + required > MAX_SEND_QUEUE {

                    self.dropped_frames = self.dropped_frames.saturating_add(1);
                    if self.last_drop_log.elapsed().as_secs() >= 1 {
                        tracing::info!(
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
            client.state_frame_start = None;
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
}
