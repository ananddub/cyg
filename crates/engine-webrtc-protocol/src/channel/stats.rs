use super::assembler::FragmentAssembler;

impl FragmentAssembler {
    pub fn stale(&self) -> u64 {
        self.stale
    }

    pub fn duplicates(&self) -> u64 {
        self.duplicates
    }

    pub fn rejected(&self) -> u64 {
        self.rejected
    }

    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    pub fn assembled(&self) -> u64 {
        self.assembled
    }

    pub fn inflight(&self) -> usize {
        self.inflight.len()
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffered
    }

    pub fn newest_seq(&self) -> Option<u16> {
        self.newest
    }
}
