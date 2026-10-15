use super::assembler::FragmentAssembler;

/// Counters, kept out of the reassembly logic's file.
impl FragmentAssembler {
    /// Chunks discarded because a newer delta had already been delivered.
    pub fn stale(&self) -> u64 {
        self.stale
    }

    /// Chunks that repeated a `frag_idx` already held. Re-delivery is expected
    /// from SCTP; a large count is a peer resending, which it must not do.
    pub fn duplicates(&self) -> u64 {
        self.duplicates
    }

    /// Frames refused by the receiver's own rules: over-MTU chunk, conflicting
    /// geometry for a held seq, or no buffer left to give.
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

    pub fn inflight(&self) -> usize {
        self.inflight.len()
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffered
    }

    /// The newest seq this assembler has handed to the game.
    pub fn newest_seq(&self) -> Option<u16> {
        self.newest
    }
}
