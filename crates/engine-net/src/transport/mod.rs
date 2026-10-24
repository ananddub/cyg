//! # Wire transports
//!
//! One folder per transport, plus the [`GameTransport`] trait that
//! [`create_transport`] selects through [`TransportType`]:
//!
//! - `udp`: raw non-blocking length-prefixed datagrams
//! - `ws`: the OpenOxide/Colyseus WebSocket wire protocol (the browser plane)
//! - `webrtc`: sans-IO `str0m` DataChannels and their channel contract
//!
//! Only `udp` and `ws` implement [`GameTransport`]; the WebRTC server is driven
//! directly by the game loop because what it moves is framed deltas, not packets.
//! Either way the simulation above this module does not know which wire a client
//! reached the room on.

pub mod udp;
pub mod webrtc;
pub mod ws;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::framing::OpCode;

pub use udp::UdpTransport;
pub use webrtc::{WebRtcServer, WebRtcServerEvent, WebRtcTransport};
pub use ws::WsTransport;

/// Supported transport implementations that sit behind [`GameTransport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Raw high-frequency non-blocking UDP datagrams.
    Udp,
    /// WebSocket with OpenOxide SDK wire-protocol compatibility (zero client migration).
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

/// Incoming transport events dispatched to the game simulation.
///
/// `client_id` is an [`Arc<str>`]: per-event/per-packet clones are refcount
/// bumps instead of string allocations.
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

/// Pluggable transport trait implemented by all network layers.
pub trait GameTransport: Send {
    /// Poll for network events with a bounded microsecond timeout, appending
    /// them to a caller-owned buffer. Reusing one `Vec` across polls keeps the
    /// hot loop allocation-free.
    fn poll_events(&mut self, timeout: Duration, events: &mut Vec<TransportEvent>) -> io::Result<()>;

    /// Send a message to a specific client session.
    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    /// Send the same message to several sessions. Transports may encode once
    /// for all recipients; the default preserves per-session behaviour.
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

    /// Broadcast a message to all connected clients.
    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    /// Politely disconnect a client (server-initiated leave). Default: no-op.
    fn disconnect(&mut self, _client_id: &str) -> io::Result<()> {
        Ok(())
    }

    /// Server-initiated close with an explicit RFC 6455 close code and reason,
    /// e.g. `close_code::WITH_ERROR` when a join is rejected. Default: falls
    /// back to [`GameTransport::disconnect`].
    fn disconnect_with(&mut self, client_id: &str, _code: u16, _reason: &str) -> io::Result<()> {
        self.disconnect(client_id)
    }

    /// Flush batched pending writes to operating system sockets.
    fn flush(&mut self) -> io::Result<()>;

    /// Current number of concurrent connected users (CCU).
    fn ccu(&self) -> usize;

    /// Bound socket address.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// Transport identifier name.
    fn transport_name(&self) -> &'static str;
}

/// Factory function to instantiate the selected transport dynamically.
pub fn create_transport(
    transport_type: TransportType,
    bind_addr: SocketAddr,
) -> io::Result<Box<dyn GameTransport>> {
    match transport_type {
        TransportType::Udp => Ok(Box::new(UdpTransport::new(bind_addr)?)),
        TransportType::Ws => Ok(Box::new(WsTransport::new(bind_addr)?)),
    }
}
