use crate::MAX_FRAME;
use crate::bytes::{le_u16, le_u32};

pub const OP_DELTA: u8 = 0x04;

pub const DELTA_HEADER_LEN: usize = 9;

pub const MAX_DELTA_CHUNK: usize = MAX_FRAME - DELTA_HEADER_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaHeader {
    pub seq: u16,
    pub tick: u32,
    pub frag_idx: u8,

    pub frag_total: u8,
}

impl DeltaHeader {
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

    #[inline]
    pub fn is_single(&self) -> bool {
        self.frag_total == 1
    }
}

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

pub fn delta_fragment_count(body_len: usize) -> Option<u8> {
    if body_len > crate::channel::MAX_DELTA_BODY {
        return None;
    }
    let count = body_len.div_ceil(MAX_DELTA_CHUNK).max(1);
    u8::try_from(count).ok()
}
