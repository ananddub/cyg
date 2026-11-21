#[derive(Debug)]
pub(crate) struct PartialDelta {
    pub seq: u16,
    pub tick: u32,
    pub total: u8,

    /// Dense backing buffer, allocated lazily on the first insert. The old
    /// eager `total * MAX_DELTA_CHUNK` preallocation let a single hostile
    /// header (or a trickle of fragments) pin the whole buffer up front.
    buf: Vec<u8>,

    present: Vec<bool>,
    pub received: usize,
    pub bytes: usize,
    pub last_ms: u64,
}

impl PartialDelta {
    #[inline]
    pub fn new(seq: u16, tick: u32, total: u8) -> Self {
        Self {
            seq,
            tick,
            total,
            buf: Vec::new(),
            present: vec![false; total as usize],
            received: 0,
            bytes: 0,
            last_ms: 0,
        }
    }

    #[inline]
    pub fn insert(&mut self, frag_idx: u8, chunk: &[u8]) -> bool {
        let idx = frag_idx as usize;
        if idx >= self.total as usize || self.present[idx] {
            return false;
        }

        // Validate on every insert: only the final fragment may be shorter
        // than MAX_DELTA_CHUNK; anything else would shift the reassembly
        // boundaries and corrupt the assembled body.
        let is_last = idx + 1 == self.total as usize;
        if !is_last && chunk.len() != crate::channel::MAX_DELTA_CHUNK {
            return false;
        }

        let max_chunk = crate::channel::MAX_DELTA_CHUNK;
        if self.buf.is_empty() {
            self.buf = vec![0u8; self.total as usize * max_chunk];
        }
        let offset = idx * max_chunk;
        self.buf[offset..offset + chunk.len()].copy_from_slice(chunk);

        self.present[idx] = true;
        self.received += 1;
        self.bytes += chunk.len();
        true
    }

    #[inline]
    pub fn is_complete(&self) -> bool {
        self.received == self.total as usize
    }

    pub fn assemble(self) -> Vec<u8> {
        let mut buf = self.buf;
        buf.truncate(self.bytes);
        buf
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledDelta {
    pub seq: u16,
    pub tick: u32,
    pub body: Vec<u8>,
}
