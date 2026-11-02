use std::collections::HashMap;

mod buffer;

#[cfg(test)]
mod tests;

pub const DEFAULT_ROLLBACK_WINDOW: usize = 64;

#[inline]
pub fn compute_checksum_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    let mut chunks = bytes.chunks_exact(8);
    for word in chunks.by_ref() {
        hash ^= u64::from_le_bytes(word.try_into().unwrap());
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for &byte in chunks.remainder() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotFrame {
    pub tick: u64,
    pub memory: Vec<u8>,
    pub checksum: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesyncReport {
    pub tick: u64,
    pub expected_checksum: u64,
    pub client_checksum: u64,
}

#[derive(Debug)]
pub struct RollbackRingBuffer {
    capacity: usize,
    frames: Vec<Option<SnapshotFrame>>,
    latest_tick: Option<u64>,
    blobs: HashMap<u64, Vec<u8>>,
}

impl Default for RollbackRingBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_ROLLBACK_WINDOW)
    }
}
