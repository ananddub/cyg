/// One delta waiting for the rest of its chunks.
///
/// Two layouts, same idx-order-concat semantics:
/// - **Flat** (the production fast path): one pre-reserved body buffer written
///   in place at `frag_idx * MAX_DELTA_CHUNK` — no per-fragment heap
///   allocations, and `assemble` hands the buffer over instead of copying into
///   a third allocation. Valid because the packer emits full chunks except the
///   last, so every chunk's offset is known on arrival.
/// - **Sparse**: the wire format permits variable chunk sizes (pinned browser
///   vectors exercise them), so a short non-final chunk migrates the partial
///   to per-fragment storage and assembles by concatenation at completion.
#[derive(Debug)]
pub(crate) struct PartialDelta {
    pub seq: u16,
    pub tick: u32,
    pub total: u8,
    /// Flat body; chunk `i` lives at `[i * MAX_DELTA_CHUNK ..]`.
    buf: Vec<u8>,
    /// Per-fragment storage once variable geometry forces sparse mode.
    sparse: Option<Vec<Option<Vec<u8>>>>,
    present: Vec<bool>,
    pub received: usize,
    pub bytes: usize,
    pub last_ms: u64,
}

impl PartialDelta {
    #[inline]
    pub fn new(seq: u16, tick: u32, total: u8) -> Self {
        let total = total as usize;
        Self {
            seq,
            tick,
            total: total as u8,
            buf: vec![0u8; total * crate::channel::MAX_DELTA_CHUNK],
            sparse: None,
            present: vec![false; total],
            received: 0,
            bytes: 0,
            last_ms: 0,
        }
    }

    /// Write one chunk. Returns `false` on a duplicate (already present).
    #[inline]
    pub fn insert(&mut self, frag_idx: u8, chunk: &[u8]) -> bool {
        let idx = frag_idx as usize;
        if self.present[idx] {
            return false;
        }

        let is_last = idx + 1 == self.total as usize;
        if self.sparse.is_none() && !is_last && chunk.len() != crate::channel::MAX_DELTA_CHUNK {
            // Variable geometry: migrate what is already flat into sparse
            // storage, then continue there for the rest of this delta.
            // In flat mode every non-final cell is exactly one chunk wide, so
            // only the final cell can be short — recover its true length from
            // the byte count.
            let max_chunk = crate::channel::MAX_DELTA_CHUNK;
            let last = self.total as usize - 1;
            let last_len = if self.present[last] {
                self.bytes - (self.received - 1) * max_chunk
            } else {
                0
            };
            let mut parts: Vec<Option<Vec<u8>>> = (0..self.total as usize)
                .map(|i| {
                    if !self.present[i] {
                        None
                    } else if i == last {
                        Some(self.buf[i * max_chunk..i * max_chunk + last_len].to_vec())
                    } else {
                        Some(self.buf[i * max_chunk..(i + 1) * max_chunk].to_vec())
                    }
                })
                .collect();
            parts[idx] = Some(chunk.to_vec());
            self.sparse = Some(parts);
        }

        match &mut self.sparse {
            Some(parts) => parts[idx] = Some(chunk.to_vec()),
            None => {
                let offset = idx * crate::channel::MAX_DELTA_CHUNK;
                self.buf[offset..offset + chunk.len()].copy_from_slice(chunk);
            }
        }

        self.present[idx] = true;
        self.received += 1;
        self.bytes += chunk.len();
        true
    }

    #[inline]
    pub fn is_complete(&self) -> bool {
        self.received == self.total as usize
    }

    /// Bodies concatenated in `frag_idx` order, not arrival order.
    pub fn assemble(self) -> Vec<u8> {
        match self.sparse {
            Some(parts) => {
                let mut body = Vec::with_capacity(self.bytes);
                for part in parts.into_iter().flatten() {
                    body.extend_from_slice(&part);
                }
                body
            }
            None => {
                // The buffer was written at fixed offsets, so completion makes
                // it the body itself — hand it over, no third copy.
                let mut buf = self.buf;
                buf.truncate(self.bytes);
                buf
            }
        }
    }
}

/// A reassembled delta, ready to hand to whatever parses the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledDelta {
    pub seq: u16,
    pub tick: u32,
    pub body: Vec<u8>,
}
