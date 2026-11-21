use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::WebRtcTransport;
use crate::framing::OpCode;
use crate::transport::webrtc::contract::Lane;
use crate::transport::{GameTransport, TransportEvent};

impl GameTransport for WebRtcTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> io::Result<()> {
        events.append(&mut self.pending);

        self.events.clear();
        self.poll.poll(&mut self.events, Some(timeout))?;
        self.recv_udp();
        let now = Instant::now();

        self.server.tick(now);

        for (dest, data) in self.server.poll_transmits() {
            let _ = self.sock.send_to(&data, dest);
        }

        self.dispatch_server_events(now);
        self.dispatch_inbound(now);

        self.sweep_stale(now);

        events.append(&mut self.pending);
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        match opcode {
            OpCode::StateDelta | OpCode::StateSnapshot => {
                let seq = self.next_delta_seq(client_id);
                let _ = self.server.send_delta(client_id, seq, 0, payload);
            }
            _ => {
                self.server.send_reliable(client_id, payload);
            }
        }
        self.flush()
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let lane = match opcode {
            OpCode::StateDelta | OpCode::StateSnapshot => Lane::Unreliable,
            _ => Lane::Reliable,
        };
        self.server.broadcast_frame(lane, payload);
        self.flush()
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        if self.server.remove_session(client_id) {
            self.last_seen.remove(client_id);
            self.delta_seqs.remove(client_id);
            if !self.pending.iter().any(|e| matches!(e, TransportEvent::Disconnected { client_id: cid } if cid.as_ref() == client_id)) {
                self.pending.push(TransportEvent::Disconnected {
                    client_id: client_id.into(),
                });
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_transmits()
    }

    fn ccu(&self) -> usize {
        self.last_seen.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    fn transport_name(&self) -> &'static str {
        "WebRTC DataChannel (UDP/str0m)"
    }
}
