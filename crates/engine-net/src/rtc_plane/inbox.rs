
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::oneshot;

use super::types::RtcPlaneError;

pub(crate) enum SessionInbound {
    Datagram {
        from: SocketAddr,
        bytes: Arc<[u8]>,
    },
    Delta {
        seq: u16,
        tick: u32,
        body: Arc<[u8]>,
    },
    Reliable {
        body: Arc<[u8]>,
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
