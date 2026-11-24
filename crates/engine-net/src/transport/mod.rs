pub mod batch;
pub mod gso;
pub mod quic_transport;
#[cfg(unix)]
mod socket;
#[cfg(test)]
mod tests;
pub mod udp;
pub mod webrtc;
pub mod ws;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::framing::OpCode;

pub use batch::{BatchSend, send_datagram_batch};
pub use gso::{gso_run_len, send_gso};
pub use quic_transport::QuicTransport;
pub use udp::UdpTransport;
pub use webrtc::{WebRtcServer, WebRtcServerEvent, WebRtcTransport};
pub use ws::WsTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Udp,
    Ws,
}

impl std::str::FromStr for TransportType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(Self::Udp),
            "colyseus" | "colyseus-ws" | "ws" => Ok(Self::Ws),
            _ => Err(format!("Unknown transport type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransportEvent {
    Connected {
        client_id: Arc<str>,
        peer_addr: SocketAddr,
    },
    Disconnected {
        client_id: Arc<str>,
    },
    Packet {
        client_id: Arc<str>,
        opcode: OpCode,
        payload: Bytes,
    },
}

pub trait GameTransport: Send {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> io::Result<()>;

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    fn send_to_many(
        &mut self,
        client_ids: &[&str],
        opcode: OpCode,
        payload: &[u8],
    ) -> io::Result<()> {
        for client_id in client_ids {
            self.send_to(client_id, opcode, payload)?;
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    fn disconnect(&mut self, _client_id: &str) -> io::Result<()> {
        Ok(())
    }

    fn disconnect_with(&mut self, client_id: &str, _code: u16, _reason: &str) -> io::Result<()> {
        self.disconnect(client_id)
    }

    fn flush(&mut self) -> io::Result<()>;

    fn ccu(&self) -> usize;

    fn local_addr(&self) -> io::Result<SocketAddr>;

    fn transport_name(&self) -> &'static str;
}

pub fn create_transport(
    transport_type: TransportType,
    bind_addr: SocketAddr,
) -> io::Result<Box<dyn GameTransport>> {
    match transport_type {
        TransportType::Udp => Ok(Box::new(UdpTransport::new(bind_addr)?)),
        TransportType::Ws => Ok(Box::new(WsTransport::new(bind_addr)?)),
    }
}
