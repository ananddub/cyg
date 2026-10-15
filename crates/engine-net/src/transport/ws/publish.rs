//! Server-initiated messages: the `ERROR` and `ROOM_RESPONSE` frames and the
//! immediate disconnect.

use std::io;
use std::sync::Arc;

use bytes::BufMut;

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
    pub(super) fn queue_to(&mut self, client_id: &str, colyseus_opcode: u8, payload: &[u8]) {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(client) = self.clients.get_mut(&token) {
                encode_wire_frame(colyseus_opcode, payload, &mut client.send_queue);
            }
        }
    }

    /// Drop a client immediately: sends the WS close frame (`4000 CONSENTED`)
    /// and surfaces a `Disconnected` event on the next `poll_events`.
    pub fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(mut client) = self.clients.remove(&token) {
                queue_close_frame(&mut client, close_code::CONSENTED, "server disconnect");
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
