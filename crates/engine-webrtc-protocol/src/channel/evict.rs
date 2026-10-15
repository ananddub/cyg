use super::{AssembledDelta, FRAGMENT_TTL_MS, MAX_INFLIGHT_BYTES, MAX_INFLIGHT_DELTAS};
use crate::channel::is_newer;

impl super::FragmentAssembler {
    /// Delivery supersedes: every in-flight delta at or behind `seq` can never
    /// be applied, so its chunks are released instead of waiting for their TTL.
    pub(super) fn deliver(&mut self, seq: u16, tick: u32, body: Vec<u8>) -> AssembledDelta {
        self.newest = Some(seq);
        self.assembled += 1;
        let mut keep = 0;
        while keep < self.inflight.len() {
            if is_newer(seq, self.inflight[keep].seq) {
                keep += 1;
            } else {
                self.drop_slot(keep);
            }
        }
        AssembledDelta { seq, tick, body }
    }

    /// Free slot and budget for one more chunk, discarding the in-flight delta
    /// furthest behind the newest thing seen.
    pub(super) fn make_room(&mut self, incoming: usize) {
        while self.inflight.len() >= MAX_INFLIGHT_DELTAS || self.needs_budget(incoming) {
            let Some(oldest) = self.oldest_slot() else {
                return;
            };
            self.drop_slot(oldest);
        }
    }

    pub(super) fn drop_slot(&mut self, idx: usize) {
        let partial = self.inflight.remove(idx);
        self.buffered -= partial.bytes;
        self.evicted += 1;
    }

    fn needs_budget(&self, incoming: usize) -> bool {
        !self.inflight.is_empty() && self.buffered + incoming > MAX_INFLIGHT_BYTES
    }

    fn oldest_slot(&self) -> Option<usize> {
        let highest = self.highest?;
        let mut oldest = 0;
        let mut behind = 0;
        for (idx, partial) in self.inflight.iter().enumerate() {
            let distance = highest.wrapping_sub(partial.seq);
            if distance > behind {
                behind = distance;
                oldest = idx;
            }
        }
        Some(oldest)
    }

    /// Forgets in-flight deltas waiting longer than [`FRAGMENT_TTL_MS`], leaving
    /// the ones still making progress alone.
    pub fn cleanup(&mut self, now_ms: u64) -> bool {
        let before = self.inflight.len();
        let mut freed = 0;
        self.inflight.retain(|partial| {
            let expired = now_ms.saturating_sub(partial.last_ms) > FRAGMENT_TTL_MS;
            if expired {
                freed += partial.bytes;
            }
            !expired
        });
        let dropped = (before - self.inflight.len()) as u64;
        if dropped != 0 {
            self.buffered -= freed;
            self.evicted += dropped;
        }
        dropped != 0
    }
}
