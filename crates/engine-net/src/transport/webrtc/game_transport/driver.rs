use std::io;
use std::time::Instant;

use super::WebRtcTransport;
use crate::framing::OpCode;
use crate::transport::webrtc::contract::{Inbound, Lane};
use crate::transport::{send_datagram_batch, send_gso, TransportEvent, WebRtcServerEvent};

impl WebRtcTransport {
    pub(super) fn dispatch_server_events(&mut self, now: Instant) {
        for ev in self.server.poll_events() {
            match ev {
                WebRtcServerEvent::Connected { client_id } => {
                    self.last_seen.insert(client_id.clone(), now);
                    if let Ok(peer) = self.sock.local_addr() {
                        self.pending.push(TransportEvent::Connected {
                            client_id: client_id.into(),
                            peer_addr: peer,
                        });
                    }
                }
                WebRtcServerEvent::ChannelOpen { client_id, label } => {
                    self.last_seen.insert(client_id.clone(), now);
                    if label == Lane::Reliable.label() && !self.schema_descriptor.is_empty() {
                        self.server
                            .send_reliable(&client_id, &self.schema_descriptor);
                    }
                }
                WebRtcServerEvent::Disconnected { client_id } => {
                    self.last_seen.remove(&client_id);
                    if !self.pending.iter().any(|e| matches!(e, TransportEvent::Disconnected { client_id: cid } if cid.as_ref() == client_id)) {
                        self.pending.push(TransportEvent::Disconnected { client_id: client_id.into() });
                    }
                }
                WebRtcServerEvent::Message { .. } => {}
            }
        }
    }

    pub(super) fn dispatch_inbound(&mut self, now: Instant) {
        for (client_id, msg) in self.server.poll_inbound() {
            self.last_seen.insert(client_id.clone(), now);
            match &msg {
                Inbound::Input { payload, .. } => {
                    self.pending.push(TransportEvent::Packet {
                        client_id: client_id.clone().into(),
                        opcode: OpCode::PlayerInput,
                        payload: bytes::Bytes::copy_from_slice(payload),
                    });
                }
                Inbound::App { data } => {
                    self.pending.push(TransportEvent::Packet {
                        client_id: client_id.clone().into(),
                        opcode: OpCode::RoomDataBytes,
                        payload: bytes::Bytes::copy_from_slice(data),
                    });
                }
                _ => {}
            }
            self.inbound.push((client_id, msg));
        }
    }

    pub(super) fn sweep_stale(&mut self, now: Instant) {
        let idle_after = self.idle_after;
        let stale: Vec<String> = self
            .last_seen
            .iter()
            .filter(|(_, seen)| now.duration_since(**seen) > idle_after)
            .map(|(cid, _)| cid.clone())
            .collect();
        for cid in stale {
            self.server.remove_session(&cid);
            self.last_seen.remove(&cid);
            if !self.pending.iter().any(|e| matches!(e, TransportEvent::Disconnected { client_id } if client_id.as_ref() == cid)) {
                self.pending.push(TransportEvent::Disconnected { client_id: cid.into() });
            }
        }
    }

    pub(super) fn flush_transmits(&mut self) -> io::Result<()> {
        let batch = self.server.poll_transmits();
        if batch.is_empty() {
            return Ok(());
        }

        let mut i = 0usize;
        let mut plain = Vec::new();
        while i < batch.len() {
            let run = crate::transport::gso_run_len(&batch, i);
            if run >= 3 {
                let (dest, _) = &batch[i];
                let seg = batch[i].1.len();
                let slices: Vec<&[u8]> =
                    batch[i..i + run].iter().map(|(_, d)| d.as_ref()).collect();
                if send_gso(&self.sock, dest, &slices, seg).is_err() {
                    send_datagram_batch(&self.sock, &batch[i..i + run]);
                }
            } else {
                plain.extend_from_slice(&batch[i..i + run]);
            }
            i += run;
        }
        if !plain.is_empty() {
            send_datagram_batch(&self.sock, &plain);
        }
        Ok(())
    }
}
