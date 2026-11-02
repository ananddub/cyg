
use std::net::SocketAddr;

use str0m::channel::ChannelId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneChannel {
    Unreliable,
    Reliable,
}

#[derive(Debug)]
pub enum Outbound {

    Datagram {
        destination: SocketAddr,
        contents: Vec<u8>,
    },

    Data {
        channel: ChannelId,
        binary: bool,
        payload: Vec<u8>,
    },

    Event(SessionEvent),
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Connected,
    Closed,
    ChannelOpen {
        id: ChannelId,
        label: String,
    },
    ChannelClose {
        id: ChannelId,
    },

    RemoteAddrBound {
        addr: SocketAddr,
    },
}
