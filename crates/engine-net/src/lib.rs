//! # Engine Net
//!
//! Low-level QUIC network listener, connection tracking, and packet multiplexer
//! for unreliable datagrams (movement/inputs) and reliable streams (state/events).

use std::net::SocketAddr;

/// Channel delivery mode for network messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    /// Reliable, ordered stream (WebSockets / QUIC stream equivalent).
    Reliable,
    /// Unreliable, unordered datagram (UDP / WebTransport datagram equivalent).
    Unreliable,
}

/// Incoming network event from a connected client.
#[derive(Debug)]
pub enum NetworkEvent {
    Connected {
        peer: SocketAddr,
        session_id: engine_core::SessionId,
    },
    Data {
        session_id: engine_core::SessionId,
        channel: DeliveryMode,
        payload: Vec<u8>,
    },
    Disconnected {
        session_id: engine_core::SessionId,
    },
}
