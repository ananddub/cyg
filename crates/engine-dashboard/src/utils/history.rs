use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use engine_bus::StatusSnapshot;

pub const HISTORY_LEN: usize = 300;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub snapshot: StatusSnapshot,
    pub arrived_ms: u64,
}

pub struct DashboardHistory {
    inner: Mutex<VecDeque<HistoryEntry>>,
    epoch: Instant,
}

impl DashboardHistory {
    pub fn new() -> DashboardHistory {
        DashboardHistory { inner: Mutex::new(VecDeque::with_capacity(HISTORY_LEN)), epoch: Instant::now() }
    }

    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn push(&self, snapshot: StatusSnapshot, arrived_ms: u64) {
        let mut q = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if q.len() == HISTORY_LEN {
            q.pop_front();
        }
        q.push_back(HistoryEntry { snapshot, arrived_ms });
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn newest(&self) -> Option<HistoryEntry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).back().cloned()
    }

    pub fn snapshot_all(&self) -> Vec<HistoryEntry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

impl Default for DashboardHistory {
    fn default() -> Self {
        DashboardHistory::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_the_newest_history_len_samples() {
        let h = DashboardHistory::new();
        for i in 0..(HISTORY_LEN + 50) {
            h.push(StatusSnapshot { seq: i as u64, ..Default::default() }, i as u64);
        }
        assert_eq!(h.len(), HISTORY_LEN);
        assert_eq!(h.newest().unwrap().snapshot.seq, (HISTORY_LEN + 49) as u64);
        assert_eq!(h.snapshot_all().first().unwrap().snapshot.seq, 50);
    }
}
