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
//! ## No retransmission, in the type
//!
//! The unreliable channel's contract is *replace, not resend*: `[`DeltaPacker`]`
//! mints a fresh room-wide `seq` per delta and keeps no copy of what it emitted,
//! and [`FragmentAssembler::is_superseded`] answers "can this still matter?" with
//! one wrap-safe compare. A receiver that wants an older delta cannot ask for it,
//! so nothing here stores it — the only recovery for a lost fragment is the next
//! tick. That is why a rejected body leaves the sequence counter untouched (no
//! gaps to reason about) and why there is no NACK, no resend queue and no
//! per-packet bitmap to keep in sync with SCTP's own.
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

use crate::MAX_FRAME;
// One wrap-safe rule for the whole protocol, re-exported here so the star's two
// halves cannot drift onto a second implementation of "is this the newest?".
pub use crate::frames::is_newer;

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

/// How many deltas one receiver may hold half-assembled at once.
///
/// `ordered: false` interleaves the tail of delta N with the head of delta N+1,
/// so a single slot throws away deltas that were perfectly recoverable — and the
/// loss rate compounds: at 10 % per fragment a 5-fragment delta needs one slot
/// *per* in-flight tick to close any of them. The bound is what stops a client
/// from paying per connected peer rather than per room.
pub const MAX_INFLIGHT_DELTAS: usize = 8;

/// Bytes of fragment bodies one receiver may buffer. Fragments arrive with a
/// declared `frag_total`, so a peer that sprays headers cannot make the server
/// reserve for a delta it will never finish; this caps the reserve that matters.
pub const MAX_INFLIGHT_BYTES: usize = 1 << 20;

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
    /// declared geometry is impossible (`idx >= total`, `total == 0`) or whose
    /// chunk cannot have fitted in one packet, because a fragment bigger than the
    /// MTU was fragmented by SCTP and is therefore not a delta we produced.
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
        if chunk.len() > MAX_DELTA_CHUNK || buf.len() > MAX_FRAME {
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

/// True when a receiver that already holds `newest` must throw `seq` away: a
/// duplicate, or anything behind it in wrap-safe order. One compare, no state —
/// this is the whole cost of the newest-wins rule.
#[inline]
pub fn is_superseded(newest: u16, seq: u16) -> bool {
    !is_newer(newest, seq)
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

/// The one way a body can fail to be packable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    /// The body needs more fragments than a `u8` index can name. Never truncate:
    /// a half-sent delta is a corrupt world state, a dropped one is last tick's.
    BodyTooLarge {
        body_len: usize,
        fragments_needed: usize,
        max_body_len: usize,
    },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BodyTooLarge {
                body_len,
                fragments_needed,
                max_body_len,
            } => write!(
                f,
                "delta body of {body_len} B needs {fragments_needed} fragments, \
                 over {MAX_FRAGMENTS}; the channel carries at most {max_body_len} B"
            ),
        }
    }
}

impl std::error::Error for PackError {}

/// One fragment of a packed delta: its stamped header and its slice of the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaFragment<'a> {
    pub header: DeltaHeader,
    pub chunk: &'a [u8],
}

impl<'a> DeltaFragment<'a> {
    /// Bytes this fragment occupies on the wire.
    #[inline]
    pub fn wire_len(&self) -> usize {
        DELTA_HEADER_LEN + self.chunk.len()
    }

    /// Append the frame to `out` without clearing it, so a broadcast loop builds
    /// a whole tick's fragments in one buffer and hands out slices.
    pub fn append_to(&self, out: &mut Vec<u8>) {
        out.push(OP_DELTA);
        out.extend_from_slice(&self.header.seq.to_le_bytes());
        out.extend_from_slice(&self.header.tick.to_le_bytes());
        out.push(self.header.frag_idx);
        out.push(self.header.frag_total);
        out.extend_from_slice(self.chunk);
    }

    /// Own a copy, for a caller that queues frames instead of sending inline.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.wire_len());
        self.append_to(&mut out);
        out
    }
}

/// A delta body stamped and split to the MTU, borrowing the caller's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaPacking<'a> {
    seq: u16,
    tick: u32,
    body: &'a [u8],
    total: u8,
}

impl<'a> DeltaPacking<'a> {
    /// Split `body` now, send later. Fails rather than emitting a truncated delta.
    pub fn new(seq: u16, tick: u32, body: &'a [u8]) -> Result<Self, PackError> {
        let Some(total) = delta_fragment_count(body.len()) else {
            return Err(PackError::BodyTooLarge {
                body_len: body.len(),
                fragments_needed: body.len().div_ceil(MAX_DELTA_CHUNK),
                max_body_len: MAX_DELTA_BODY,
            });
        };
        Ok(Self {
            seq,
            tick,
            body,
            total,
        })
    }

    #[inline]
    pub fn seq(&self) -> u16 {
        self.seq
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// `frag_total` for every fragment of this delta.
    #[inline]
    pub fn frag_total(&self) -> u8 {
        self.total
    }

    #[inline]
    pub fn body(&self) -> &'a [u8] {
        self.body
    }

    #[inline]
    pub fn body_len(&self) -> usize {
        self.body.len()
    }

    /// Total wire bytes for all fragments — the size to reserve once per tick.
    #[inline]
    pub fn wire_len(&self) -> usize {
        DELTA_HEADER_LEN * self.total as usize + self.body.len()
    }

    /// The exact fragment list, in send order.
    #[inline]
    pub fn fragments(&self) -> Fragments<'a> {
        Fragments {
            seq: self.seq,
            tick: self.tick,
            total: self.total,
            rest: self.body,
            idx: 0,
        }
    }

    /// Append every fragment to `out` and return how many were written, so a
    /// sender can slice them back out without a per-fragment allocation.
    pub fn append_frames_to(&self, out: &mut Vec<u8>) -> usize {
        let before = out.len();
        out.reserve(self.wire_len());
        for fragment in self.fragments() {
            fragment.append_to(out);
        }
        debug_assert_eq!(out.len() - before, self.wire_len());
        self.total as usize
    }
}

/// Iterator over a packed delta's fragments.
#[derive(Debug, Clone)]
pub struct Fragments<'a> {
    seq: u16,
    tick: u32,
    total: u8,
    rest: &'a [u8],
    idx: u8,
}

impl<'a> Iterator for Fragments<'a> {
    type Item = DeltaFragment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let chunk = &self.rest[..self.rest.len().min(MAX_DELTA_CHUNK)];
        self.rest = &self.rest[chunk.len()..];
        let fragment = DeltaFragment {
            header: DeltaHeader {
                seq: self.seq,
                tick: self.tick,
                frag_idx: self.idx,
                frag_total: self.total,
            },
            chunk,
        };
        self.idx += 1;
        Some(fragment)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = (self.total - self.idx) as usize;
        (left, Some(left))
    }
}

impl ExactSizeIterator for Fragments<'_> {}

/// Pack a delta body into its fragments. `DeltaPacking` borrows the body, so a
/// 128 Hz loop calls this once per tick and reuses one scratch buffer to write.
pub fn pack_delta(seq: u16, tick: u32, body: &[u8]) -> Result<DeltaPacking<'_>, PackError> {
    DeltaPacking::new(seq, tick, body)
}

/// The same split as owned frames, for a caller that hands each fragment to a
/// queue rather than a socket. Allocates one `Vec` per fragment.
pub fn pack_delta_frames(seq: u16, tick: u32, body: &[u8]) -> Result<Vec<Vec<u8>>, PackError> {
    let packing = DeltaPacking::new(seq, tick, body)?;
    Ok(packing.fragments().map(|f| f.to_vec()).collect())
}

/// The send-side half of the codec: the room-wide sequence counter plus the
/// packer that stamps it. It holds no copy of anything it has emitted, because
/// the unreliable channel's contract is to be superseded, not resent.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeltaPacker {
    seq: u16,
    tick: u32,
    published: u64,
    refused: u64,
}

impl DeltaPacker {
    /// `first_seq` is what the peer's assembler has not seen yet; starting at 0
    /// is fine for a fresh room.
    pub fn new(first_seq: u16, tick: u32) -> Self {
        Self {
            seq: first_seq,
            tick,
            ..Self::default()
        }
    }

    /// The seq the next [`Self::pack`] will stamp.
    #[inline]
    pub fn next_seq(&self) -> u16 {
        self.seq
    }

    #[inline]
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// Follow `room.current_tick`; the seq is what orders deltas, the tick is
    /// what lets the client interpolate, so both move together.
    #[inline]
    pub fn set_tick(&mut self, tick: u32) {
        self.tick = tick;
    }

    /// Stamp `body` and advance the sequence, wrapping at 65536.
    pub fn pack<'b>(&mut self, body: &'b [u8]) -> Result<DeltaPacking<'b>, PackError> {
        let packing = DeltaPacking::new(self.seq, self.tick, body)?;
        self.seq = self.seq.wrapping_add(1);
        self.published += 1;
        Ok(packing)
    }

    /// Stamp and send only if the body fits, counting the delta it dropped. A
    /// refused delta does not consume a seq, so receivers never see a gap they
    /// have to explain.
    pub fn pack_or_drop<'b>(&mut self, body: &'b [u8]) -> Option<DeltaPacking<'b>> {
        match self.pack(body) {
            Ok(packing) => Some(packing),
            Err(_) => {
                self.refused += 1;
                None
            }
        }
    }

    /// Deltas stamped and handed to the transport.
    #[inline]
    pub fn published(&self) -> u64 {
        self.published
    }

    /// Deltas too large to carry. Non-zero means the guest outran the channel,
    /// not the network.
    #[inline]
    pub fn refused(&self) -> u64 {
        self.refused
    }
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
    seq: u16,
    tick: u32,
    total: u8,
    parts: Vec<Option<Vec<u8>>>,
    received: usize,
    bytes: usize,
    last_ms: u64,
}

impl PartialDelta {
    #[inline]
    fn is_complete(&self) -> bool {
        self.received == self.total as usize
    }

    /// Bodies concatenated in `frag_idx` order, not arrival order.
    fn assemble(self) -> Vec<u8> {
        let mut body = Vec::with_capacity(self.bytes);
        for part in self.parts.iter().flatten() {
            body.extend_from_slice(part);
        }
        body
    }
}

/// A reassembled delta, ready to hand to whatever parses the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledDelta {
    pub seq: u16,
    pub tick: u32,
    pub body: Vec<u8>,
}

/// Reassembles fragmented deltas, dropping anything the game can no longer use.
///
/// Feed every [`DeltaHeader`] in arrival order with [`Self::insert`] — or a raw
/// datagram with [`Self::insert_frame`] — and it returns `Some` exactly once per
/// delta, when its last chunk closes it. A chunk for a `seq` already superseded is
/// counted in [`Self::stale`] and ignored: that single wrap-safe compare is what
/// keeps `parts` from growing forever when a client loses one chunk of a tick
/// nobody cares about anymore.
///
/// Supersession is measured against the newest *delivered* seq rather than the
/// newest chunk seen, because `ordered: false` interleaves the tail of delta N
/// with the head of delta N+1 — an arrival that merely *starts* a newer delta
/// says nothing about whether N will complete. So a bounded window of deltas is
/// held at once, whichever order their chunks arrive in.
#[derive(Debug, Default)]
pub struct FragmentAssembler {
    /// Newest delta handed to the game. Anything at or behind it is stale.
    newest: Option<u16>,
    /// Newest seq ever accepted, whole or not: the origin the in-flight window is
    /// measured from, so eviction can name the delta that is furthest behind.
    highest: Option<u16>,
    inflight: Vec<PartialDelta>,
    buffered: usize,
    stale: u64,
    duplicates: u64,
    rejected: u64,
    evicted: u64,
    assembled: u64,
}

impl FragmentAssembler {
    /// `now_ms` is any monotonic millisecond clock the caller has; the codec
    /// crate deliberately does not own a clock.
    pub fn insert(
        &mut self,
        header: &DeltaHeader,
        chunk: &[u8],
        now_ms: u64,
    ) -> Option<AssembledDelta> {
        if chunk.len() > MAX_DELTA_CHUNK {
            self.rejected += 1;
            return None;
        }
        // `insert` is public and takes a header the caller may have built itself,
        // so the geometry is checked here too rather than only in `parse`.
        if header.frag_total == 0 || header.frag_idx >= header.frag_total {
            self.rejected += 1;
            return None;
        }
        if self.is_superseded(header.seq) {
            self.stale += 1;
            return None;
        }
        if self.highest.is_none_or(|h| is_newer(h, header.seq)) {
            self.highest = Some(header.seq);
        }

        if header.is_single() {
            return Some(self.deliver(header.seq, header.tick, chunk.to_vec()));
        }

        let slot = match self.inflight.iter().position(|p| p.seq == header.seq) {
            Some(pos) => pos,
            None => {
                self.make_room(chunk.len());
                if self.buffered + chunk.len() > MAX_INFLIGHT_BYTES {
                    // Nothing left to evict and the window is full: this peer is
                    // spraying half-deltas, so it gets no more buffer.
                    self.rejected += 1;
                    return None;
                }
                self.inflight.push(PartialDelta {
                    seq: header.seq,
                    tick: header.tick,
                    total: header.frag_total,
                    parts: vec![None; header.frag_total as usize],
                    received: 0,
                    bytes: 0,
                    last_ms: now_ms,
                });
                self.inflight.len() - 1
            }
        };

        // Two bodies claiming one seq would join into a delta that is a fragment
        // list of neither, so the slot keeps the geometry it declared first.
        let conflicts = {
            let held = &self.inflight[slot];
            held.tick != header.tick || held.total != header.frag_total
        };
        if conflicts {
            self.rejected += 1;
            self.drop_slot(slot);
            return None;
        }

        let mut duplicate = false;
        let mut complete = false;
        {
            let partial = &mut self.inflight[slot];
            partial.last_ms = now_ms;
            let cell = &mut partial.parts[header.frag_idx as usize];
            if cell.is_some() {
                duplicate = true;
            } else {
                let len = chunk.len();
                *cell = Some(chunk.to_vec());
                partial.received += 1;
                partial.bytes += len;
                self.buffered += len;
                complete = partial.is_complete();
            }
        }
        if duplicate {
            self.duplicates += 1;
            return None;
        }
        if !complete {
            return None;
        }

        let partial = self.inflight.remove(slot);
        self.buffered -= partial.bytes;
        Some(self.deliver(partial.seq, partial.tick, partial.assemble()))
    }

    /// Parse a received datagram and feed it in. A frame that is not a well-formed
    /// delta is counted in [`Self::rejected`] instead of being misassembled.
    pub fn insert_frame(&mut self, frame: &[u8], now_ms: u64) -> Option<AssembledDelta> {
        match DeltaHeader::parse(frame) {
            Some((header, chunk)) => self.insert(&header, chunk, now_ms),
            None => {
                self.rejected += 1;
                None
            }
        }
    }

    /// Would a receiver in this state throw `seq` away? Free of allocation and of
    /// the in-flight table, so a caller can filter before it copies anything.
    #[inline]
    pub fn is_superseded(&self, seq: u16) -> bool {
        match self.newest {
            Some(newest) => crate::channel::is_superseded(newest, seq),
            None => false,
        }
    }

    /// Delivery supersedes: once the game holds `seq`, every in-flight delta at or
    /// behind it can never be applied, so its chunks are released immediately
    /// instead of waiting for their TTL.
    fn deliver(&mut self, seq: u16, tick: u32, body: Vec<u8>) -> AssembledDelta {
        self.newest = Some(seq);
        self.assembled += 1;
        let mut keep = 0;
        while keep < self.inflight.len() {
            if is_newer(seq, self.inflight[keep].seq) {
                keep += 1;
            } else {
                self.drop_slot(keep);
            }
        }
        AssembledDelta { seq, tick, body }
    }

    /// Free slot and budget for one more chunk, discarding the in-flight delta
    /// furthest behind the newest thing seen. A delta that old was superseded in
    /// all but name the moment the tick counter moved past it.
    fn make_room(&mut self, incoming: usize) {
        while self.inflight.len() >= MAX_INFLIGHT_DELTAS || self.needs_budget(incoming) {
            let Some(oldest) = self.oldest_slot() else { return };
            self.drop_slot(oldest);
        }
    }

    fn needs_budget(&self, incoming: usize) -> bool {
        !self.inflight.is_empty() && self.buffered + incoming > MAX_INFLIGHT_BYTES
    }

    /// Index of the in-flight delta furthest behind [`Self::highest`], which every
    /// entry trails by construction because `highest` only moves forward.
    fn oldest_slot(&self) -> Option<usize> {
        let highest = self.highest?;
        let mut oldest = 0;
        let mut behind = 0;
        for (idx, partial) in self.inflight.iter().enumerate() {
            let distance = highest.wrapping_sub(partial.seq);
            if distance > behind {
                behind = distance;
                oldest = idx;
            }
        }
        Some(oldest)
    }

    fn drop_slot(&mut self, idx: usize) {
        let partial = self.inflight.remove(idx);
        self.buffered -= partial.bytes;
        self.evicted += 1;
    }

    /// Forgets in-flight deltas that have been waiting longer than
    /// [`FRAGMENT_TTL_MS`], leaving the ones still making progress alone. Returns
    /// whether anything was dropped. A delta superseded by a newer one is dropped
    /// by [`Self::insert`] instead, so this only fires for a peer that has gone
    /// quiet entirely — the one leak this layer cannot close by rule alone.
    pub fn cleanup(&mut self, now_ms: u64) -> bool {
        let before = self.inflight.len();
        let mut freed = 0;
        self.inflight.retain(|partial| {
            let expired = now_ms.saturating_sub(partial.last_ms) > FRAGMENT_TTL_MS;
            if expired {
                freed += partial.bytes;
            }
            !expired
        });
        let dropped = (before - self.inflight.len()) as u64;
        if dropped != 0 {
            self.buffered -= freed;
            self.evicted += dropped;
        }
        dropped != 0
    }

    /// Chunks discarded because a newer delta had already been delivered.
    pub fn stale(&self) -> u64 {
        self.stale
    }

    /// Chunks that repeated a `frag_idx` already held. A re-delivery is expected
    /// from SCTP; a large count is a peer resending, which it must not do.
    pub fn duplicates(&self) -> u64 {
        self.duplicates
    }

    /// Frames refused by the receiver's own rules: a chunk over the MTU, a seq
    /// whose declared geometry contradicts the delta already in the slot, or a
    /// delta that could not be given budget.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    /// Partial deltas released before completing, by supersession, eviction or TTL.
    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    /// Deltas delivered complete.
    pub fn assembled(&self) -> u64 {
        self.assembled
    }

    /// Deltas currently half-received.
    pub fn inflight(&self) -> usize {
        self.inflight.len()
    }

    /// Bytes of fragment bodies being held. Bounded by [`MAX_INFLIGHT_BYTES`].
    pub fn buffered_bytes(&self) -> usize {
        self.buffered
    }

    /// The newest seq this assembler has handed to the game.
    pub fn newest_seq(&self) -> Option<u16> {
        self.newest
    }
}
