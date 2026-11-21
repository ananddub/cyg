use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::oneshot;

use super::types::RtcPlaneError;

pub(crate) enum SessionInbound {
    Datagram {
        from: SocketAddr,
        // Bytes, not Arc<[u8]>: one alloc per inbound datagram either way,
        // but Bytes has no atomic refcount — cloning to fan out to unbound
        // sessions is a pointer bump, and the demux loop allocates one of
        // these per packet at 60Hz × N peers.
        bytes: Bytes,
    },
    Delta {
        seq: u16,
        tick: u32,
        body: Bytes,
    },
    Reliable {
        body: Bytes,
    },
    Answer {
        sdp: String,
    },
    Candidate {
        sdp: String,
    },
    Close,
}

pub(crate) enum PlaneCommand {
    CreateSession {
        session_id: Arc<str>,
        player_id: Arc<str>,
        reply: oneshot::Sender<Result<String, RtcPlaneError>>,
    },

    Forward {
        session_id: Arc<str>,
        msg: SessionInbound,
    },
    Shutdown,
}

pub(crate) enum SessionNotice {
    RemoteAddrBound {
        session_id: Arc<str>,
        addr: SocketAddr,
    },
    Gone {
        session_id: Arc<str>,
    },
}
