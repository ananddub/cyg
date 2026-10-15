/// One delta waiting for the rest of its chunks.
#[derive(Debug)]
pub(crate) struct PartialDelta {
    pub seq: u16,
    pub tick: u32,
    pub total: u8,
    pub parts: Vec<Option<Vec<u8>>>,
    pub received: usize,
    pub bytes: usize,
    pub last_ms: u64,
}

impl PartialDelta {
    #[inline]
    pub fn is_complete(&self) -> bool {
        self.received == self.total as usize
    }

    /// Bodies concatenated in `frag_idx` order, not arrival order.
    pub fn assemble(self) -> Vec<u8> {
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
