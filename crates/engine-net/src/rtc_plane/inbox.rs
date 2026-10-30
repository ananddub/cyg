//! The plane's internal message envelopes: inbox items, control commands and
//! session notices. None of these cross the public surface.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::oneshot;

use super::types::RtcPlaneError;

/// Bounded mailbox every datagram/producer hands to a single session task.
/// `Datagram` and `Delta` share an `Arc<[u8]>` so a broadcast does not clone the
/// payload once per session.
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

/// Cold-path control from [`PlaneHandle`](super::PlaneHandle) into the demux task.
pub(crate) enum PlaneCommand {
    CreateSession {
        session_id: Arc<str>,
        player_id: Arc<str>,
        reply: oneshot::Sender<Result<String, RtcPlaneError>>,
    },
    /// `apply_answer` / `add_candidate` / `close_session` resolve the session id
    /// to its inbox inside the demux (the sole owner of the live session map).
    Forward {
        session_id: Arc<str>,
        msg: SessionInbound,
    },
    Shutdown,
}

/// Session→demux notices (remote-address binding / teardown).
pub(crate) enum SessionNotice {
    RemoteAddrBound {
        session_id: Arc<str>,
        addr: SocketAddr,
    },
    Gone {
        session_id: Arc<str>,
    },
}
