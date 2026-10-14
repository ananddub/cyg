//! Server ⇄ client channel layer: sequence numbers, fragmentation and acks.
//!
//! The mesh frames in [`crate::frames`] are peer-to-peer and tiny by
//! construction. This module is the other side of the star: the **server**
//! pushing its per-tick delta to every client over `game-unreliable`, where the
//! payload size is whatever the guest decided to emit and loss is expected.
//!
//! Three problems, three mechanisms, all copied from the part of lightyear's
//! transport that survives contact with SCTP:
//!
//! 1. **Out-of-order delivery.** `game-unreliable` is `{ ordered: false,
//!    maxRetransmits: 0 }`, so tick 41 can land before tick 40. A [`DeltaHeader`]
//!    carries a room-wide `seq`, and a receiver keeps only the newest one —
//!    [`is_newer`], the same wrap-safe rule the mesh frames use.
//! 2. **One datagram, one packet.** WebRTC's path MTU is ~1200 bytes. A delta
//!    bigger than that would be fragmented by SCTP, where losing any chunk kills
//!    the whole message *and* holds reassembly state for it. We fragment
//!    ourselves so a stale message's chunks can be dropped on arrival by rule 1.
//! 3. **Proof of receipt.** [`Ack`] exists so the server can tell whether a
//!    client actually holds the event it was promised. SCTP already resends on
//!    the reliable channel, so this is a delivery *record*, not a retransmit
//!    trigger — that distinction matters and is not hidden here.
//!
//! ## Why no ack bitfield
//!
//! lightyear tracks 33 packets per ack because it owns reliability over raw UDP
//! (its 13-byte packet header: `last_ack_packet_id` + a `u32` bitmask). Over a
//! DataChannel, SCTP already numbers, orders and retransmits, so an
//! application-level packet ledger would only duplicate the transport's own
//! state machine and be right less often than it. The unreliable channel does not
//! want per-packet acks either: a frame nobody has is worth less than the next
//! frame that just arrived. What is borrowed instead is the *wrap-safe
//! comparison* and the fragment constructor with a 3-second expiry.

use crate::frames::is_newer;
use crate::MAX_FRAME;

/// Server → client, `game-unreliable`: `[op][seq u16][tick u32][idx u8][total u8][chunk]`.
pub const OP_DELTA: u8 = 0x04;

/// Client → server, `game-reliable`: `[op][seq u16][kind u8]`.
pub const OP_ACK: u8 = 0x06;

/// `DeltaHeader` size: `[op 1][seq 2][tick 4][idx 1][total 1]`.
pub const DELTA_HEADER_LEN: usize = 9;

/// Wire length of an [`Ack`].
pub const ACK_WIRE_LEN: usize = 4;

/// Bytes of delta body that fit in one frame, derived the way lightyear derives
/// its fragment size: subtract the header from the path-MTU budget rather than
/// hard-coding a second constant that can drift.
pub const MAX_DELTA_CHUNK: usize = MAX_FRAME - DELTA_HEADER_LEN;

/// `frag_total` is a `u8`, and reassembling more than this many chunks for a
/// tick nobody wants is how a stuck client turns packet loss into a memory leak.
pub const MAX_FRAGMENTS: usize = 255;

/// The largest delta body the channel layer will carry. Bigger than
/// `engine_wasm::MAX_OUTBOUND_DELTA_SIZE`, so the ABI limit is always the
/// binding one.
pub const MAX_DELTA_BODY: usize = MAX_FRAGMENTS * MAX_DELTA_CHUNK;

/// How long a partially received delta may wait for the rest. A delta is
/// superseded within a tick or two; anything older is a leak, not a message.
pub const FRAGMENT_TTL_MS: u64 = 3000;

/// What an [`Ack`] confirms. Only events live on the reliable channel today;
/// the tick that the checkpoint store will use lands in Phase B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AckKind {
    Event = 1,
}

impl AckKind {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Event),
            _ => None,
        }
    }
}

#[inline]
fn le_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

#[inline]
fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The stamp on one fragment of one server delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaHeader {
    /// Room-wide, incremented once per delta the server emits. Same for every
    /// recipient of that tick, so `seq` orders the *world*, not the socket.
    pub seq: u16,
    /// `room.current_tick` truncated to `u32`. Free at 1191 bytes per frame, and
    /// interpolation cannot be built later without a wire break.
    pub tick: u32,
    /// Zero-based index of this chunk within the delta.
    pub frag_idx: u8,
    /// How many chunks this delta was split into. `1` means unfragmented.
    pub frag_total: u8,
}

impl DeltaHeader {
    /// Borrow the header and the chunk it points at. Rejects a frame whose
    /// declared geometry is impossible (`idx >= total`, `total == 0`).
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        let (&op, rest) = buf.split_first()?;
        if op != OP_DELTA || rest.len() < DELTA_HEADER_LEN - 1 {
            return None;
        }
        let header = Self {
            seq: le_u16(rest, 0),
            tick: le_u32(rest, 2),
            frag_idx: rest[6],
            frag_total: rest[7],
        };
        let chunk = &rest[DELTA_HEADER_LEN - 1..];
        if header.frag_total == 0 || header.frag_idx >= header.frag_total {
            return None;
        }
        if header.frag_total == 1 && (header.frag_idx != 0 || chunk.len() > MAX_DELTA_CHUNK) {
            return None;
        }
        Some((header, chunk))
    }

    /// This delta is complete once every chunk has arrived.
    #[inline]
    pub fn is_single(&self) -> bool {
        self.frag_total == 1
    }
}

/// Write one delta frame into a caller-owned buffer, so a broadcast loop at 128
/// Hz reuses one allocation.
pub fn write_delta_frame(
    out: &mut Vec<u8>,
    seq: u16,
    tick: u32,
    frag_idx: u8,
    frag_total: u8,
    chunk: &[u8],
) {
    out.clear();
    out.reserve(DELTA_HEADER_LEN + chunk.len());
    out.push(OP_DELTA);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&tick.to_le_bytes());
    out.push(frag_idx);
    out.push(frag_total);
    out.extend_from_slice(chunk);
}

/// How many chunks a body of this length needs. `None` means the body cannot be
/// carried — the caller should drop the delta, not silently truncate it.
pub fn delta_fragment_count(body_len: usize) -> Option<u8> {
    if body_len > MAX_DELTA_BODY {
        return None;
    }
    let count = body_len.div_ceil(MAX_DELTA_CHUNK).max(1);
    u8::try_from(count).ok()
}

/// `[0x06][seq u16][kind u8]`.
pub fn write_ack(out: &mut Vec<u8>, seq: u16, kind: AckKind) {
    out.clear();
    out.reserve(ACK_WIRE_LEN);
    out.push(OP_ACK);
    out.extend_from_slice(&seq.to_le_bytes());
    out.push(kind as u8);
}

/// A client confirming it holds a reliable frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ack {
    pub seq: u16,
    pub kind: AckKind,
}

/// `Some` only for an exactly-sized ack frame, so a longer guest payload that
/// happens to start with `0x06` is never mistaken for one.
pub fn parse_ack(buf: &[u8]) -> Option<Ack> {
    if buf.len() != ACK_WIRE_LEN || buf[0] != OP_ACK {
        return None;
    }
    Some(Ack {
        seq: le_u16(buf, 1),
        kind: AckKind::from_u8(buf[3])?,
    })
}

/// One delta waiting for the rest of its chunks.
#[derive(Debug)]
struct PartialDelta {
    tick: u32,
    total: u8,
    parts: Vec<Option<Vec<u8>>>,
    received: usize,
    last_ms: u64,
}

/// A reassembled delta, ready to hand to whatever parses the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledDelta {
    pub seq: u16,
    pub tick: u32,
    pub body: Vec<u8>,
}

/// Reassembles fragmented deltas, dropping anything older than the newest one it
/// has seen.
///
/// Feed every [`DeltaHeader`] in arrival order with [`Self::insert`]; it returns
/// `Some` exactly once per delta, when the last chunk closes it. A chunk for a
/// `seq` already superseded is counted in [`Self::stale`] and ignored — that is
/// the whole point of the layer, and it is what keeps `parts` from growing
/// forever when a client loses one chunk of a tick nobody cares about anymore.
#[derive(Debug, Default)]
pub struct FragmentAssembler {
    newest: Option<u16>,
    partial: Option<(u16, PartialDelta)>,
    stale: u64,
    assembled: u64,
}

impl FragmentAssembler {
    /// `now_ms` is any monotonic millisecond clock the caller has; the codec
    /// crate deliberately does not own a clock.
    ///
    /// Interleaving is handled by supersession, not by buffering several
    /// messages at once: if chunks of tick 10 arrive while tick 11 is
    /// half-assembled, tick 10 wins the slot and tick 11 is discarded. Out-of-order
    /// within one delta is fine (`frag_idx` places each chunk); out-of-order
    /// *between* deltas is what this layer exists to throw away.
    pub fn insert(
        &mut self,
        header: &DeltaHeader,
        chunk: &[u8],
        now_ms: u64,
    ) -> Option<AssembledDelta> {
        if let Some(last) = self.newest {
            // Already delivered, or superseded. Only a strictly newer seq opens
            // a message, so a re-delivered duplicate cannot apply twice.
            if header.seq == last || !is_newer(last, header.seq) {
                self.stale += 1;
                return None;
            }
        }

        if header.is_single() {
            self.newest = Some(header.seq);
            self.assembled += 1;
            self.partial = None;
            return Some(AssembledDelta {
                seq: header.seq,
                tick: header.tick,
                body: chunk.to_vec(),
            });
        }

        // A new message supersedes whatever half-built thing we were holding:
        // the older one cannot become interesting again.
        if self.partial.as_ref().map(|(seq, _)| *seq) != Some(header.seq) {
            self.partial = Some((
                header.seq,
                PartialDelta {
                    tick: header.tick,
                    total: header.frag_total,
                    parts: vec![None; header.frag_total as usize],
                    received: 0,
                    last_ms: now_ms,
                },
            ));
        }
        let (_, partial) = self.partial.as_mut().expect("partial just installed");
        partial.last_ms = now_ms;
        let slot = &mut partial.parts[header.frag_idx as usize];
        if slot.is_none() {
            *slot = Some(chunk.to_vec());
            partial.received += 1;
        }
        if partial.received != partial.total as usize {
            return None;
        }

        let mut body = Vec::new();
        for part in partial.parts.iter() {
            if let Some(bytes) = part {
                body.extend_from_slice(bytes);
            }
        }
        let tick = partial.tick;
        self.newest = Some(header.seq);
        self.partial = None;
        self.assembled += 1;
        Some(AssembledDelta {
            seq: header.seq,
            tick,
            body,
        })
    }

    /// Forgets a partial delta that has been waiting longer than
    /// [`FRAGMENT_TTL_MS`]. Returns whether anything was dropped. A delta that
    /// is superseded by a newer one is dropped by [`Self::insert`] instead, so
    /// this only fires for a receiver that has gone quiet entirely.
    pub fn cleanup(&mut self, now_ms: u64) -> bool {
        let expired = self
            .partial
            .as_ref()
            .is_some_and(|(_, p)| now_ms.saturating_sub(p.last_ms) > FRAGMENT_TTL_MS);
        if expired {
            self.partial = None;
        }
        expired
    }

    /// Chunks discarded because a newer delta had already arrived.
    pub fn stale(&self) -> u64 {
        self.stale
    }

    /// Deltas delivered complete.
    pub fn assembled(&self) -> u64 {
        self.assembled
    }

    /// The newest seq this assembler has fully or partially accepted.
    pub fn newest_seq(&self) -> Option<u16> {
        self.newest
    }
}
