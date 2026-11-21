//! One datagram, one IP packet.
//!
//! The UDP lane used to hand `wrap_full` straight to `send_to`. A `send_to` above the path MTU
//! does not fail: it fragments at the IP layer, and then a single lost fragment loses the whole
//! state update — with no way for the client to ask for it again, because the lane is
//! unreliable. On a path that forbids fragmentation it fails outright with `EMSGSIZE` and the
//! session silently stops receiving state.
//!
//! Everything the UDP lane sends therefore goes through [`send`]: at or below the MTU it is the
//! same single datagram as before; above it, the frame is split with the delta-fragment header
//! the client already reassembles (`cyg_core::wire::DeltaAssembler`), so each datagram stays a
//! whole packet.

use std::net::{SocketAddr, UdpSocket};

use engine_webrtc_protocol::{MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame};

/// Payload budget for one datagram. Matches `engine_webrtc_protocol::MAX_FRAME`, so both lanes
/// agree on what "one packet" means, and leaves room for IPv6 + UDP headers inside 1280.
pub(crate) const UDP_MTU: usize = engine_webrtc_protocol::MAX_FRAME;

/// Sends `frame` to `peer`, fragmenting when it does not fit in one datagram.
///
/// `wire` accumulates the bytes actually written and the number of frames that had to be split,
/// so a caller can report bandwidth without re-measuring.
///
/// `seq` groups the fragments of one frame and `tick` lets the receiver expire a half-assembled
/// set; both are unchanged for every fragment of `frame`. Returns the number of datagrams
/// written, or `None` when the frame is too large to fragment at all (the caller logs it).
pub(crate) fn send(
    sock: &UdpSocket,
    peer: SocketAddr,
    frame: &[u8],
    seq: u16,
    tick: u32,
    scratch: &mut Vec<u8>,
) -> Option<usize> {
    send_counted(
        sock,
        peer,
        frame,
        seq,
        tick,
        scratch,
        &mut WireCount::default(),
    )
}

/// [`send`] with the counters the lane reports.
pub(crate) fn send_counted(
    sock: &UdpSocket,
    peer: SocketAddr,
    frame: &[u8],
    seq: u16,
    tick: u32,
    scratch: &mut Vec<u8>,
    wire: &mut WireCount,
) -> Option<usize> {
    if frame.len() <= UDP_MTU {
        if sock.send_to(frame, peer).is_ok() {
            wire.bytes += frame.len() as u64;
        }
        return Some(1);
    }

    let total = delta_fragment_count(frame.len())?;
    wire.fragmented_frames += 1;
    for (idx, chunk) in frame.chunks(MAX_DELTA_CHUNK).enumerate() {
        write_delta_frame(scratch, seq, tick, idx as u8, total, chunk);
        if sock.send_to(scratch.as_slice(), peer).is_ok() {
            wire.bytes += scratch.len() as u64;
            wire.datagrams += 1;
        }
    }
    Some(total as usize)
}

/// What one [`send`] actually put on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireCount {
    pub bytes: u64,
    pub datagrams: u64,
    /// Frames that did not fit in one datagram and were split.
    pub fragmented_frames: u64,
}

#[cfg(test)]
mod tests;
