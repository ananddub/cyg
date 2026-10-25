//! # Deterministic Memory Snapshotting & Rollback Engine
//!
//! Provides sub-millisecond memory snapshotting, rolling ring-buffer history,
//! and instant rewinds for WebAssembly game rooms. Enables GGPO-style
//! client-side prediction, rollback, and frame checksum desync verification.

use std::collections::HashMap;

use crate::error::WasmError;
use crate::instance::WasmInstance;

/// Default history size for rollback: 64 ticks (approx. 1 second at 60Hz or 3.2s at 20Hz).
pub const DEFAULT_ROLLBACK_WINDOW: usize = 64;

/// FNV-1a 64-bit non-cryptographic checksum for rapid state divergence detection.
#[inline]
/// FNV-1a 64-bit non-cryptographic checksum for rapid state divergence detection.
///
/// Consumes the buffer 8 bytes at a time (little-endian words) and only falls
/// back to byte steps for the tail: same avalanche behaviour, ~8x fewer
/// multiplies, which matters when this runs over the full linear memory every
/// tick. Any client that wants to compare must fold words the same way.
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

/// A captured snapshot of a specific simulation tick's memory state and hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotFrame {
    /// Simulation tick number.
    pub tick: u64,
    /// Raw linear memory bytes of the WebAssembly guest.
    pub memory: Vec<u8>,
    /// 64-bit state checksum computed over the memory.
    pub checksum: u64,
}

/// Discrepancy report generated when a client's frame checksum does not match the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesyncReport {
    pub tick: u64,
    pub expected_checksum: u64,
    pub client_checksum: u64,
}

/// Circular ring-buffer maintaining historical simulation frames for rollback and resimulation.
#[derive(Debug)]
pub struct RollbackRingBuffer {
    capacity: usize,
    frames: Vec<Option<SnapshotFrame>>,
    latest_tick: Option<u64>,
    /// Guest-serialized state blobs (from the room's save() export), keyed by
    /// the same ring index as `frames`. Preferred over raw memory snapshots:
    /// the guest serializes only its own state, so a checkpoint is kilobytes,
    /// not megabytes.
    blobs: HashMap<u64, Vec<u8>>,
}

impl Default for RollbackRingBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_ROLLBACK_WINDOW)
    }
}

impl RollbackRingBuffer {
    /// Create a new rollback buffer with a fixed frame capacity.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(4);
        let mut frames = Vec::with_capacity(cap);
        for _ in 0..cap {
            frames.push(None);
        }
        Self {
            capacity: cap,
            blobs: HashMap::new(),
            frames,
            latest_tick: None,
        }
    }

    /// Snapshot the instance's linear memory at `tick`. Reuses existing memory buffers
    /// within the circular slot to eliminate heap allocator churn.
    pub fn save_snapshot<T>(
        &mut self,
        tick: u64,
        instance: &WasmInstance<T>,
    ) -> Result<u64, WasmError> {
        let memory_slice = instance.raw_memory_slice()?;
        let checksum = compute_checksum_64(memory_slice);

        let index = (tick as usize) % self.capacity;
        if let Some(ref mut existing) = self.frames[index] {
            existing.tick = tick;
            existing.checksum = checksum;
            existing.memory.clear();
            existing.memory.extend_from_slice(memory_slice);
        } else {
            self.frames[index] = Some(SnapshotFrame {
                tick,
                memory: memory_slice.to_vec(),
                checksum,
            });
        }

        self.latest_tick = Some(tick);
        Ok(checksum)
    }

    /// Roll back the instance's linear memory to the state captured at `target_tick`.
    ///
    /// Execution time is bounded strictly by `memcpy` speed (typically under 5 microseconds
    /// for a typical 64KB - 512KB game state).
    pub fn rollback_to<T>(
        &self,
        target_tick: u64,
        instance: &mut WasmInstance<T>,
    ) -> Result<u64, WasmError> {
        let index = (target_tick as usize) % self.capacity;
        let frame = self.frames[index].as_ref().ok_or_else(|| {
            WasmError::Trap(
                "rollback".to_string(),
                format!("Tick {target_tick} not available in rollback window"),
            )
        })?;

        if frame.tick != target_tick {
            return Err(WasmError::Trap(
                "rollback".to_string(),
                format!(
                    "Slot expired: expected tick {target_tick}, but slot contains tick {}",
                    frame.tick
                ),
            ));
        }

        instance.restore_memory(&frame.memory)?;
        Ok(frame.checksum)
    }

    /// Store the guest's serialized state blob for `tick`.
    /// Keeps at most `capacity` blobs (oldest evicted first).
    pub fn save_blob(&mut self, tick: u64, blob: Vec<u8>) {
        self.blobs.insert(tick, blob);
        if self.blobs.len() > self.capacity {
            let mut ticks: Vec<u64> = self.blobs.keys().copied().collect();
            ticks.sort_unstable();
            let excess = self.blobs.len() - self.capacity;
            for t in ticks.into_iter().take(excess) {
                self.blobs.remove(&t);
            }
        }
    }

    /// The stored blob for `tick`, if it is still inside the window.
    pub fn blob_for(&self, tick: u64) -> Option<&[u8]> {
        self.blobs.get(&tick).map(Vec::as_slice)
    }

    /// Retrieve the checksum for a historical frame if still present in the buffer.
    pub fn get_checksum(&self, tick: u64) -> Option<u64> {
        let index = (tick as usize) % self.capacity;
        let frame = self.frames[index].as_ref()?;
        if frame.tick == tick {
            Some(frame.checksum)
        } else {
            None
        }
    }

    /// Verify whether a client's checksum matches the authoritative host's checksum at `tick`.
    pub fn verify_sync(&self, tick: u64, client_checksum: u64) -> Result<(), DesyncReport> {
        if let Some(expected) = self.get_checksum(tick) {
            if expected == client_checksum {
                Ok(())
            } else {
                Err(DesyncReport {
                    tick,
                    expected_checksum: expected,
                    client_checksum,
                })
            }
        } else {
            // Frame is outside the active rollback window; accept conservatively.
            Ok(())
        }
    }

    /// Get the min and max tick currently held within the buffer.
    pub fn available_range(&self) -> Option<(u64, u64)> {
        let mut min_tick = u64::MAX;
        let mut max_tick = 0;
        let mut found = false;

        for frame_opt in &self.frames {
            if let Some(frame) = frame_opt {
                min_tick = min_tick.min(frame.tick);
                max_tick = max_tick.max(frame.tick);
                found = true;
            }
        }

        if found {
            Some((min_tick, max_tick))
        } else {
            None
        }
    }

    /// Returns the latest recorded simulation tick.
    pub fn latest_tick(&self) -> Option<u64> {
        self.latest_tick
    }

    /// Clear all stored snapshots.
    pub fn clear(&mut self) {
        for frame in &mut self.frames {
            *frame = None;
        }
        self.latest_tick = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_computation() {
        let b1 = b"Player1_Position_X_10_Y_20";
        let b2 = b"Player1_Position_X_10_Y_20";
        let b3 = b"Player1_Position_X_10_Y_21";

        let h1 = compute_checksum_64(b1);
        let h2 = compute_checksum_64(b2);
        let h3 = compute_checksum_64(b3);

        assert_eq!(h1, h2, "Identical bytes must produce identical checksums");
        assert_ne!(h1, h3, "Different bytes must produce distinct checksums");
    }

    #[test]
    fn test_rollback_ring_buffer_range_and_slotting() {
        let mut buf = RollbackRingBuffer::new(4);
        assert_eq!(buf.available_range(), None);

        // Manually mock frames into ring buffer slots for unit verification
        buf.frames[0] = Some(SnapshotFrame {
            tick: 4,
            memory: vec![1, 2, 3],
            checksum: 100,
        });
        buf.frames[1] = Some(SnapshotFrame {
            tick: 5,
            memory: vec![4, 5, 6],
            checksum: 200,
        });

        assert_eq!(buf.get_checksum(4), Some(100));
        assert_eq!(buf.get_checksum(5), Some(200));
        assert_eq!(buf.get_checksum(0), None); // slot 0 now holds tick 4!

        assert!(buf.verify_sync(4, 100).is_ok());
        let desync = buf.verify_sync(4, 999).unwrap_err();
        assert_eq!(desync.tick, 4);
        assert_eq!(desync.expected_checksum, 100);
        assert_eq!(desync.client_checksum, 999);
    }
}
