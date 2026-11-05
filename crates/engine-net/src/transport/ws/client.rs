
use std::sync::Arc;
use std::time::Instant;

use bytes::BytesMut;

#[derive(PartialEq)]
pub(super) enum WsState {
    Handshaking,
    Connected,
}

/// Per-client send backlog ceiling. A client that stops draining holds at
/// most this much RSS; past MAX_OVERFLOW_STRIKES consecutive overflows the
/// peer is disconnected instead of being starved forever.
pub(super) const MAX_SEND_QUEUE: usize = 256 * 1024;
pub(super) const MAX_OVERFLOW_STRIKES: u32 = 3;

pub(super) struct WsClient {
    pub(super) stream: mio::net::TcpStream,
    pub(super) state: WsState,
    pub(super) session_id: Arc<str>,

    pub(super) room_id: String,
    pub(super) recv_buf: BytesMut,
    pub(super) send_queue: BytesMut,
    /// One shared delta body per flush window: the room serializes its patch
    /// ONCE into this Arc and every draining client writes from the same
    /// allocation instead of holding a private copy. Order vs inline bytes is
    /// preserved by the pump (send_queue -> shared_frame -> post_shared).
    pub(super) shared_frame: Option<(std::sync::Arc<[u8]>, u32)>,
    pub(super) shared_pending: bool,
    /// Inline bytes that arrived AFTER the shared frame was queued; flushed
    /// into send_queue once the shared body has been written out.
    pub(super) post_shared: BytesMut,

    pub(super) frag_opcode: Option<u8>,
    pub(super) frag_buf: BytesMut,
    pub(super) last_activity: Instant,
    pub(super) ping_sent_at: Option<Instant>,
    pub(super) missed_pings: u32,

    pub(super) consented: bool,
    pub(super) overflow_strikes: u32,

    pub(super) state_frame_start: Option<usize>,
    pub(super) state_frame_len: usize,

    pub(super) registered_writable: bool,
    pub(super) delayed_pings: Vec<(Instant, bytes::Bytes)>,
}
