//! # Unified Multi-Transport Abstraction
//!
//! Provides a single pluggable `GameTransport` trait supporting:
//! - `TransportType::Tcp`: Raw ultra-low-overhead length-prefixed TCP
//! - `TransportType::Udp`: Raw high-frequency non-blocking UDP datagrams
//! - `TransportType::Quic`: QUIC multiplexed datagrams and streams
//! - `TransportType::Ws`: WebSocket wire-protocol transport (browser plane)
//!
//! The game engine core (128Hz Tick Loop, WASM Room, Rewind, Reconciliation) is 100%
//! decoupled from the network transport and works identically with any option.

pub mod ws_transport;
pub mod udp;
pub mod webrtc_frames;
pub mod webrtc_server;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::framing::OpCode;

pub use ws_transport::WsTransport;
pub use udp::UdpTransport;
pub use webrtc_server::{WebRtcServer, WebRtcServerEvent};

/// Supported transport implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Non-blocking raw TCP with length-prefixed binary frames
    /// Raw high-frequency UDP datagram transport
    Udp,
    /// QUIC multiplexed reliable streams & unreliable datagrams
    /// WebSocket with OpenOxide SDK wire-protocol compatibility (zero client migration)
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

    /// Broadcast a message to all connected clients.
    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()>;

    /// Politely disconnect a client (server-initiated leave). Default: no-op.
    fn disconnect(&mut self, _client_id: &str) -> io::Result<()> {
        Ok(())
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
