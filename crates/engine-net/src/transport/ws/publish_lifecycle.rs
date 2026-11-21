use std::io;
use std::sync::Arc;

use super::client::MAX_SEND_QUEUE;
use super::protocol::wire_opcode_for;
use super::rfc6455::{encode_wire_frame, finish_close, queue_close_frame};
use super::transport::WsTransport;
use crate::transport::TransportEvent;

impl WsTransport {
    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> io::Result<()> {
        let Some(token) = self.clients_meta.get(client_id).map(|m| m.token) else {
            if super::trace_enabled() {
                eprintln!(
                    "[t] NOMETA {client_id} keys={:?}",
                    self.clients_meta
                        .keys()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                );
            }
            return Ok(());
        };
        let Some(client) = self.clients.get_mut(&token) else {
            if super::trace_enabled() {
                eprintln!(
                    "[t] NOCLIENT {client_id} tok={token:?} clients={:?}",
                    self.clients.keys().collect::<Vec<_>>()
                );
            }
            return Ok(());
        };
        if super::trace_enabled() {
            let hex: String = payload
                .iter()
                .take(48)
                .map(|b| format!("{b:02x}"))
                .collect();
            eprintln!(
                "[t] ST {} tok={token:?} len={} q={} start={:?} hex={hex}",
                client_id,
                payload.len(),
                client.send_queue.len(),
                client.state_frame_start
            );
        }
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
        self.disconnect_with(
            client_id,
            super::protocol::close_code::CONSENTED,
            "server disconnect",
        )
    }

    pub fn disconnect_with(&mut self, client_id: &str, code: u16, reason: &str) -> io::Result<()> {
        if let Some(token) = self.clients_meta.get(client_id).map(|m| m.token) {
            if let Some(mut client) = self.clients.remove(&token) {
                queue_close_frame(&mut client, code, reason);
                let _ = self.poll.registry().deregister(&mut client.stream);
                finish_close(client);
            }
            self.clients_meta.remove(client_id);

            self.consented.insert(client_id.to_string());
            self.pending_events.push(TransportEvent::Disconnected {
                client_id: Arc::from(client_id),
            });
        }
        Ok(())
    }
}
