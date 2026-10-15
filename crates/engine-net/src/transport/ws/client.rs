//! One connected (or still-handshaking) SDK client and its mio socket.

use std::sync::Arc;
use std::time::Instant;

use bytes::BytesMut;

/// State of a connected OpenOxide SDK client.
#[derive(PartialEq)]
pub(super) enum WsState {
    Handshaking,
    Connected,
}

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
}
