use std::io::{self, IoSlice, Write};
use std::sync::Arc;
use std::time::Instant;

use bytes::Buf;

use bytes::BytesMut;

impl WsClient {
    /// Drains buffered output in ONE syscall (`write_vectored`) instead of
    /// separate `write()` calls per segment. Message ordering is the wire
    /// contract, so the iovec chain is built in exact order:
    ///   pending shared body:  [inline queue][shared body][tail]
    ///   finished shared body: [leftover tail][inline queue]  (tail FIRST —
    ///   it completes the previous message; new frames must not jump ahead)
    ///   otherwise:            [inline queue]
    /// Returns bytes written (Ok(0) = socket backpressured, not an error).
    pub(super) fn write_out(&mut self) -> io::Result<usize> {
        // Snapshot the shared body behind an Arc clone so the iovecs borrow a
        // local, not `self` — the client is advanced mutably afterwards.
        let shared = self
            .shared_frame
            .as_ref()
            .map(|(body, offset)| (Arc::clone(body), *offset as usize));

        let mut bufs: [IoSlice; 3];
        let tail_first = self.shared_frame.is_none() && !self.post_shared.is_empty();
        let shared_rest_len = shared
            .as_ref()
            .map(|(body, offset)| body.len().saturating_sub(*offset))
            .unwrap_or(0);
        let shared_rest: &[u8] = shared
            .as_ref()
            .map(|(body, offset)| &body[(*offset).min(body.len())..])
            .unwrap_or(&[]);

        if shared.is_some() {
            bufs = [
                IoSlice::new(&self.send_queue),
                IoSlice::new(shared_rest),
                IoSlice::new(&self.post_shared),
            ];
        } else if tail_first {
            bufs = [
                IoSlice::new(&self.post_shared),
                IoSlice::new(&self.send_queue),
                IoSlice::new(&[]),
            ];
        } else {
            bufs = [
                IoSlice::new(&self.send_queue),
                IoSlice::new(&[]),
                IoSlice::new(&[]),
            ];
        }
        let mut n = self.stream.write_vectored(&mut bufs)?;
        if n == 0 {
            return Ok(0);
        }

        // Advance segments in the SAME order they were offered.
        if shared.is_some() {
            let q = n.min(self.send_queue.len());
            self.send_queue.advance(q);
            n -= q;
            if let Some((body, offset)) = self.shared_frame.as_mut() {
                let s = n.min(body.len() - *offset as usize);
                *offset += s as u32;
                n -= s;
            }
            let t = n.min(self.post_shared.len());
            self.post_shared.advance(t);
            if self
                .shared_frame
                .as_ref()
                .is_some_and(|(body, offset)| *offset as usize >= body.len())
            {
                self.shared_frame = None;
            }
        } else {
            let first_len = if tail_first {
                self.post_shared.len()
            } else {
                0
            };
            let t = n.min(first_len);
            self.post_shared.advance(t);
            n -= t;
            let q = n.min(self.send_queue.len());
            self.send_queue.advance(q);
            n -= q;
        }

        if self.send_queue.is_empty() {
            self.overflow_strikes = 0;
        }
        let _ = shared_rest_len;
        Ok(n)
    }
}

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
