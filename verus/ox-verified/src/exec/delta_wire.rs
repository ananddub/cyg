// Verified mirror of `crates/engine-webrtc-protocol/src/channel/delta.rs`
// (plus the byte helpers and constants it depends on).
//
// Keep in sync with the originals:
// - crates/engine-webrtc-protocol/src/channel/delta.rs
// - crates/engine-webrtc-protocol/src/channel/mod.rs (MAX_FRAGMENTS/MAX_DELTA_BODY)
// - crates/engine-webrtc-protocol/src/lib.rs (MAX_FRAME)
// - crates/engine-webrtc-protocol/src/bytes.rs (le_u16/le_u32)

#[cfg_attr(verus, verus_verify)]
pub const MAX_FRAME: usize = 1200;
#[cfg_attr(verus, verus_verify)]
pub const OP_DELTA: u8 = 0x04;
#[cfg_attr(verus, verus_verify)]
pub const DELTA_HEADER_LEN: usize = 9;
#[cfg_attr(verus, verus_verify)]
pub const MAX_DELTA_CHUNK: usize = MAX_FRAME - DELTA_HEADER_LEN;
#[cfg_attr(verus, verus_verify)]
pub const MAX_FRAGMENTS: usize = 255;
#[cfg_attr(verus, verus_verify)]
pub const MAX_DELTA_BODY: usize = MAX_FRAGMENTS * MAX_DELTA_CHUNK;

/// Little-endian u16 read. Mirrors bytes.rs `le_u16`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        at + 1 < bytes.len(),
    ensures
        r == s_le_u16(bytes, at),
))]
pub fn le_u16(bytes: &[u8], at: usize) -> u16 {
    // identical to u16::from_le_bytes([bytes[at], bytes[at + 1]])
    ((bytes[at + 1] as u16) * 256) + (bytes[at] as u16)
}

/// Little-endian u32 read. Mirrors bytes.rs `le_u32`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    requires
        at + 3 < bytes.len(),
    ensures
        r == s_le_u32(bytes, at),
))]
pub fn le_u32(bytes: &[u8], at: usize) -> u32 {
    // identical to u32::from_le_bytes([b0, b1, b2, b3])
    ((bytes[at + 3] as u32) * 16777216)
        + ((bytes[at + 2] as u32) * 65536)
        + ((bytes[at + 1] as u32) * 256)
        + (bytes[at] as u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus, verus_verify)]
pub struct DeltaHeader {
    pub seq: u16,
    pub tick: u32,
    pub frag_idx: u8,
    pub frag_total: u8,
}

impl DeltaHeader {
    /// Parse a delta frame: `[OP_DELTA | seq:u16le | tick:u32le | frag_idx | frag_total | chunk...]`.
    ///
    /// Returns `None` on: bad opcode, short buffer, `frag_total == 0`,
    /// `frag_idx >= frag_total`, oversized chunk, or oversized frame.
    /// The returned chunk is always `<= MAX_DELTA_CHUNK` bytes.
    ///
    /// NOTE: `#[verus_spec]` cannot annotate impl methods in this Verus build,
    /// so the verified logic lives in the free fn `parse_delta_frame` and this
    /// method is a thin delegate.
    pub fn parse(buf: &[u8]) -> Option<(Self, &[u8])> {
        parse_delta_frame(buf)
    }

    #[inline]
    pub fn is_single(&self) -> bool {
        self.frag_total == 1
    }
}

/// Verified core of `DeltaHeader::parse`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(r =>
    ensures
        match r {
            Some((h, chunk)) => {
                h.frag_total >= 1
                    && h.frag_idx < h.frag_total
                    && chunk.len() <= MAX_DELTA_CHUNK as int
                    && buf.len() <= MAX_FRAME as int
                    && h.seq == s_le_u16(buf, 1)
                    && h.tick == s_le_u32(buf, 3)
            },
            None => buf.len() == 0 || buf[0] != OP_DELTA || buf.len() < DELTA_HEADER_LEN
                || buf.len() > MAX_FRAME
                || {
                    let ft = buf[8] as int;
                    let fi = buf[7] as int;
                    ft == 0 || fi >= ft || buf.len() - DELTA_HEADER_LEN > MAX_DELTA_CHUNK
                },
        }
))]
pub fn parse_delta_frame(buf: &[u8]) -> Option<(DeltaHeader, &[u8])> {
    // Same logic as the engine original (which splits off byte 0 first and
    // reads the header from the remainder); reads are expressed against `buf`
    // directly so the verified postcondition connects to `buf` indices.
    let op = *buf.first()?;
    if op != OP_DELTA || buf.len() < DELTA_HEADER_LEN {
        return None;
    }
    let header = DeltaHeader {
        seq: le_u16(buf, 1),
        tick: le_u32(buf, 3),
        frag_idx: buf[7],
        frag_total: buf[8],
    };
    let chunk = &buf[DELTA_HEADER_LEN..];
    if header.frag_total == 0 || header.frag_idx >= header.frag_total {
        return None;
    }
    if chunk.len() > MAX_DELTA_CHUNK || buf.len() > MAX_FRAME {
        return None;
    }
    Some((header, chunk))
}

/// Serialize one delta frame into `out` (cleared first).
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(
    ensures
        final(out).len() == DELTA_HEADER_LEN + chunk.len(),
        final(out)[0] == OP_DELTA,
        final(out)[1] == seq as u8,
        final(out)[2] == (seq >> 8) as u8,
        final(out)[3] == tick as u8,
        final(out)[4] == (tick >> 8) as u8,
        final(out)[5] == (tick >> 16) as u8,
        final(out)[6] == (tick >> 24) as u8,
        final(out)[7] == frag_idx,
        final(out)[8] == frag_total,
        forall|i: int| 0 <= i < chunk.len() ==> final(out)[DELTA_HEADER_LEN + i] == chunk[i],
))]
pub fn write_delta_frame(
    out: &mut Vec<u8>,
    seq: u16,
    tick: u32,
    frag_idx: u8,
    frag_total: u8,
    chunk: &[u8],
) {
    // Same bytes as the engine original (pushing little-endian halves instead
    // of `to_le_bytes`), so Verus can verify the postcondition. The header
    // byte facts are carried through the loop invariant, since Verus only
    // keeps invariant facts across a loop.
    out.clear();
    out.push(OP_DELTA);
    out.push(seq as u8);
    out.push((seq >> 8) as u8);
    out.push(tick as u8);
    out.push((tick >> 8) as u8);
    out.push((tick >> 16) as u8);
    out.push((tick >> 24) as u8);
    out.push(frag_idx);
    out.push(frag_total);
    let mut i = 0usize;
    #[cfg_attr(verus, verus_spec(
        invariant
            i <= chunk.len(),
            out.len() == DELTA_HEADER_LEN + i,
            out[0] == OP_DELTA,
            out[1] == seq as u8,
            out[2] == (seq >> 8) as u8,
            out[3] == tick as u8,
            out[4] == (tick >> 8) as u8,
            out[5] == (tick >> 16) as u8,
            out[6] == (tick >> 24) as u8,
            out[7] == frag_idx,
            out[8] == frag_total,
            forall|j: int| 0 <= j < i ==> out[DELTA_HEADER_LEN + j] == chunk[j as int],
        decreases
            chunk.len() - i,
    ))]
    while i < chunk.len() {
        out.push(chunk[i]);
        i += 1;
    }
}

/// Number of fragments needed for `body_len` bytes.
///
/// `None` when the body cannot fit in `MAX_FRAGMENTS` chunks; otherwise
/// always in `1..=MAX_FRAGMENTS`.
#[cfg_attr(verus, verus_verify)]
#[cfg_attr(verus, verus_spec(n =>
    ensures
        body_len > MAX_DELTA_BODY as int ==> n.is_none(),
        body_len <= MAX_DELTA_BODY as int ==>
            match n {
                Some(n) => 1 <= n as int && n as int <= MAX_FRAGMENTS as int,
                None => false,
            }
))]
pub fn delta_fragment_count(body_len: usize) -> Option<u8> {
    if body_len > MAX_DELTA_BODY {
        return None;
    }
    // ceil division: identical to body_len.div_ceil(MAX_DELTA_CHUNK) (the
    // engine original); written as plain arithmetic so Verus can verify it.
    let count = ((body_len + MAX_DELTA_CHUNK - 1) / MAX_DELTA_CHUNK).max(1);
    u8::try_from(count).ok()
}
