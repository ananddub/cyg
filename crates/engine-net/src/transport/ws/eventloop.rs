use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use super::client::WsState;
use super::protocol::wire_opcode_for;
use super::transport::WsTransport;
use crate::framing::OpCode;
use crate::transport::{GameTransport, TransportEvent};

impl GameTransport for WsTransport {
    fn poll_events(&mut self, timeout: Duration, ev: &mut Vec<TransportEvent>) -> io::Result<()> {
        self.pump(timeout, ev)
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        self.queue_to(client_id, wire_opcode_for(opcode), payload);
        Ok(())
    }

    fn send_to_many(
        &mut self,
        client_ids: &[&str],
        opcode: OpCode,
        payload: &[u8],
    ) -> io::Result<()> {
        self.send_many(client_ids, wire_opcode_for(opcode), payload);
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        self.pump_broadcast(wire_opcode_for(opcode), payload);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.pump_flush()
    }

    fn ccu(&self) -> usize {
        self.clients
            .values()
            .filter(|c| matches!(c.state, WsState::Connected))
            .count()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bound_addr)
    }

    fn transport_name(&self) -> &'static str {
        "WireSocket (SDK Compatible)"
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        WsTransport::disconnect(self, client_id)
    }

    fn disconnect_with(&mut self, client_id: &str, code: u16, reason: &str) -> io::Result<()> {
        WsTransport::disconnect_with(self, client_id, code, reason)
    }
}
