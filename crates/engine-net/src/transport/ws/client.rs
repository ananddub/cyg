//! One connected (or still-handshaking) SDK client and its mio socket.

use std::sync::Arc;
use std::time::Instant;

use bytes::BytesMut;

/// State of a connected CYG SDK client.
#[derive(PartialEq)]
pub(super) enum WsState {
    Handshaking,
    Connected,
}

/// Hard ceiling for one client's pending bytes. A peer that stops reading must
/// not be able to grow the server's heap: at 16 KB per queued frame this is
/// dozens of seconds of backlog, far past the point where the client is gone.
pub(super) const MAX_SEND_QUEUE: usize = 1024 * 1024;

pub(super) struct WsClient {
    pub(super) stream: mio::net::TcpStream,
    pub(super) state: WsState,
    pub(super) session_id: Arc<str>,
    /// Room id from the upgrade URL path (/processId/roomId?...).
    pub(super) room_id: String,
    pub(super) recv_buf: BytesMut,
    pub(super) send_queue: BytesMut,
    /// Inbound fragmented message accumulation (RFC 6455 continuation frames).
    pub(super) frag_opcode: Option<u8>,
    pub(super) frag_buf: BytesMut,
    pub(super) last_activity: Instant,
    pub(super) ping_sent_at: Option<Instant>,
    pub(super) missed_pings: u32,
    /// Client asked to leave (`LEAVE_ROOM` or a WS close). The host uses this
    /// to skip `allowReconnection` seat holds for intentional departures.
    pub(super) consented: bool,
    /// Start and length of the last queued schema-state frame, while it is
    /// still the unflushed tail of `send_queue` (latest-wins state channel).
    pub(super) state_frame_start: Option<usize>,
    pub(super) state_frame_len: usize,
    /// Whether the socket is currently registered with WRITABLE interest.
    /// Level-triggered epoll reports a writable socket on EVERY poll, so the
    /// interest is added only while send backpressure exists and removed the
    /// moment the queue drains — otherwise the loop burns a core per client.
    pub(super) registered_writable: bool,
}
