//! The session's outward-facing model: what one pump step can hand back.

use std::net::SocketAddr;

use str0m::channel::ChannelId;

/// The WebRTC channel a payload arrived on / should be sent on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneChannel {
    Unreliable,
    Reliable,
}

/// One step of output produced by draining the engine.
#[derive(Debug)]
pub enum Outbound {
    /// Put these bytes on the UDP socket toward `destination`.
    Datagram { destination: SocketAddr, contents: Vec<u8> },
    /// A DataChannel payload received from the remote peer.
    Data { channel: ChannelId, binary: bool, payload: Vec<u8> },
    /// A lifecycle transition the plane must react to.
    Event(SessionEvent),
}

/// Lifecycle events surfaced from the str0m engine to the session task.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Connected,
    Closed,
    ChannelOpen { id: ChannelId, label: String },
    ChannelClose { id: ChannelId },
    /// The session accepted its first packet from `addr`; the demux can now pin
    /// `addr -> session` and stop broadcasting unknown datagrams to it.
    RemoteAddrBound { addr: SocketAddr },
}
