use super::delta::{DELTA_HEADER_LEN, OP_DELTA};
use crate::channel::DeltaHeader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaFragment<'a> {
    pub header: DeltaHeader,
    pub chunk: &'a [u8],
}

impl<'a> DeltaFragment<'a> {
    #[inline]
    pub fn wire_len(&self) -> usize {
        DELTA_HEADER_LEN + self.chunk.len()
    }

    pub fn append_to(&self, out: &mut Vec<u8>) {
        out.push(OP_DELTA);
        out.extend_from_slice(&self.header.seq.to_le_bytes());
        out.extend_from_slice(&self.header.tick.to_le_bytes());
        out.push(self.header.frag_idx);
        out.push(self.header.frag_total);
        out.extend_from_slice(self.chunk);
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.wire_len());
        self.append_to(&mut out);
        out
    }
}
