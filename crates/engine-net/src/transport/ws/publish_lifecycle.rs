use std::io;
use std::sync::Arc;

use super::client::MAX_SEND_QUEUE;
use super::protocol::wire_opcode_for;
use super::rfc6455::{encode_wire_frame, finish_close, queue_close_frame};
use super::transport::WsTransport;
use crate::transport::TransportEvent;

impl WsTransport {
    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> io::Result<()> {
        let Some(&token) = self.client_tokens.get(client_id) else {
            return Ok(());
        };
        let Some(client) = self.clients.get_mut(&token) else {
            return Ok(());
        };
        let opcode = wire_opcode_for(crate::framing::OpCode::RoomDataBytes);
        if let Some(start) = client.state_frame_start {
            if start + client.state_frame_len == client.send_queue.len() {
                client.send_queue.truncate(start);
                encode_wire_frame(opcode, payload, &mut client.send_queue);
                client.state_frame_len = client.send_queue.len() - start;
                self.flush_dirty = true;
                return Ok(());
            }
            client.state_frame_start = None;
        }
        if client.send_queue.len() + payload.len() + 16 > MAX_SEND_QUEUE {
            self.dropped_frames = self.dropped_frames.saturating_add(1);
            return Ok(());
        }
        let start = client.send_queue.len();
        encode_wire_frame(opcode, payload, &mut client.send_queue);
        client.state_frame_start = Some(start);
        client.state_frame_len = client.send_queue.len() - start;
        self.flush_dirty = true;
        Ok(())
    }

    pub fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        self.disconnect_with(client_id, super::protocol::close_code::CONSENTED, "server disconnect")
    }

    pub fn disconnect_with(&mut self, client_id: &str, code: u16, reason: &str) -> io::Result<()> {
        if let Some(&token) = self.client_tokens.get(client_id) {
            if let Some(mut client) = self.clients.remove(&token) {
                queue_close_frame(&mut client, code, reason);
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
            }
            self.client_tokens.remove(client_id);
            self.client_rooms.remove(client_id);
            self.client_options.remove(client_id);
            self.seat_tokens.remove(client_id);

            self.consented.insert(client_id.to_string());
            self.pending_events.push(TransportEvent::Disconnected {
                client_id: Arc::from(client_id),
            });
        }
        Ok(())
    }
}
