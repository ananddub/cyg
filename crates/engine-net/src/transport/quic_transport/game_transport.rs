use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::Token;

use super::QuicTransport;
use crate::framing::OpCode;
use crate::transport::{GameTransport, TransportEvent};

impl GameTransport for QuicTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> io::Result<()> {
        events.append(&mut self.pending);
        let now = Instant::now();

        let wait = self
            .engine
            .next_timeout()
            .map(|at| at.saturating_duration_since(now))
            .map_or(timeout, |left| left.min(timeout));
        self.events.clear();
        self.poll.poll(&mut self.events, Some(wait))?;
        if self.events.is_empty() {}
        let now = Instant::now();
        if self.events.iter().any(|e| e.token() == Token(0)) {
            self.readable(now);
        }

        self.engine.accept_pending(now);
        if let Some(at) = self.engine.next_timeout() {
            if at <= now {
                self.engine.handle_timeout(now);
            }
        }
        self.flush_outbound(now);

        for event in self.engine.poll_events() {
            self.handle_engine_event(event);
        }
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let Some(&handle) = self.by_client.get(client_id) else {
            return Ok(());
        };
        match opcode {
            OpCode::StateDelta | OpCode::StateSnapshot => {
                let mut datagram = Vec::with_capacity(1 + payload.len());
                datagram.push(opcode as u8);
                datagram.extend_from_slice(payload);
                self.engine.datagram_send(handle, datagram);
            }

            _ => {
                let mut frame = Vec::with_capacity(1 + payload.len());
                frame.push(opcode as u8);
                frame.extend_from_slice(payload);
                self.send_frame(&Arc::<str>::from(client_id), frame);
            }
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let ids: Vec<Arc<str>> = self.by_client.keys().cloned().collect();
        for id in ids {
            self.send_to(&id, opcode, payload)?;
        }
        Ok(())
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        if let Some(&handle) = self.by_client.get(client_id) {
            self.engine
                .connection_close(Instant::now(), handle, "server leave");
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_outbound(Instant::now());
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.by_client.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    fn transport_name(&self) -> &'static str {
        "quic"
    }
}
