use std::net::SocketAddr;
use std::time::{Duration, Instant};

use engine_net::framing::OpCode;
use engine_net::transport::{GameTransport, TransportEvent};

use crate::host::*;
impl GameTransport for LoopTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> std::io::Result<()> {
        self.poll_events_inner(timeout, events)
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> std::io::Result<()> {
        if (opcode == OpCode::PlayerInput || opcode == OpCode::RoomDataBytes) && self.should_drop_packet() {
            return Ok(());
        }
        if !self.latency.is_zero() {
            self.delayed_out.push_back((
                Instant::now() + self.latency,
                DelayedOut::Op {
                    client_id: client_id.to_string(),
                    opcode,
                    payload: payload.to_vec(),
                },
            ));
            return Ok(());
        }
        self.send_to_now(client_id, opcode, payload)
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> std::io::Result<()> {
        self.rtc.broadcast(opcode, payload)?;
        if let Some(quic) = self.quic.as_mut() {
            quic.broadcast(opcode, payload)?;
        }
        if let Some(ws) = self.ws.as_mut() {
            ws.broadcast(opcode, payload)?;
        }
        Ok(())
    }

    fn disconnect(&mut self, client_id: &str) -> std::io::Result<()> {
        if self.quic.as_ref().is_some_and(|q| q.knows(client_id)) {
            return self.quic.as_mut().unwrap().disconnect(client_id);
        }
        if self.ws_knows(client_id) {
            return self.ws.as_mut().unwrap().disconnect(client_id);
        }
        self.rtc.disconnect(client_id)
    }

    fn disconnect_with(&mut self, client_id: &str, code: u16, reason: &str) -> std::io::Result<()> {
        self.consented.insert(client_id.to_string());
        if self.quic.as_ref().is_some_and(|q| q.knows(client_id)) {
            return self
                .quic
                .as_mut()
                .unwrap()
                .disconnect_with(client_id, code, reason);
        }
        if self.ws_knows(client_id) {
            self.ws
                .as_mut()
                .unwrap()
                .disconnect(client_id)?;
            return Ok(());
        }
        self.rtc.disconnect(client_id)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.rtc.flush()?;
        if let Some(quic) = self.quic.as_mut() {
            quic.flush()?;
        }
        if let Some(ws) = self.ws.as_mut() {
            ws.flush()?;
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.rtc.ccu()
            + self.quic.as_ref().map_or(0, |q| q.ccu())
            + self.ws.as_ref().map_or(0, |w| w.ccu())
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.local_addr.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no local addr")
        })
    }

    fn transport_name(&self) -> &'static str {
        "webrtc-datagram-loop"
    }
}
