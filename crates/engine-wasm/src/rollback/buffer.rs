use std::collections::HashMap;

use super::{DesyncReport, RollbackRingBuffer, SnapshotFrame, compute_checksum_64};
use crate::error::WasmError;
use crate::instance::WasmInstance;

impl RollbackRingBuffer {
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

    pub fn blob_for(&self, tick: u64) -> Option<&[u8]> {
        self.blobs.get(&tick).map(Vec::as_slice)
    }

    pub fn get_checksum(&self, tick: u64) -> Option<u64> {
        let index = (tick as usize) % self.capacity;
        if let Some(frame) = self.frames[index].as_ref() {
            if frame.tick == tick {
                return Some(frame.checksum);
            }
        }
        self.blobs.get(&tick).map(|blob| compute_checksum_64(blob))
    }

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
            Ok(())
        }
    }

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

    pub fn latest_tick(&self) -> Option<u64> {
        self.latest_tick
    }

    pub fn clear(&mut self) {
        for frame in &mut self.frames {
            *frame = None;
        }
        self.latest_tick = None;
    }
}
