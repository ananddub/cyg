use serde::{Deserialize, Serialize};

use super::{Aabb, Vec3};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalEntity {
    pub entity_id: u32,
    pub kind: u32,
    pub bounds: Aabb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickSnapshot {
    pub tick: u64,
    pub timestamp_ms: u64,
    pub entities: Vec<HistoricalEntity>,
}

#[derive(Debug, Clone)]
pub struct LagCompensator {
    pub history: std::collections::VecDeque<TickSnapshot>,
    pub max_ticks: usize,
}

impl Default for LagCompensator {
    fn default() -> Self {
        Self::new(128)
    }
}

impl LagCompensator {
    pub fn new(max_ticks: usize) -> Self {
        Self {
            history: std::collections::VecDeque::with_capacity(max_ticks),
            max_ticks: max_ticks.max(16),
        }
    }

    pub fn record_entity(&mut self, tick: u64, timestamp_ms: u64, entity: HistoricalEntity) {
        if let Some(last) = self.history.back_mut() {
            if last.tick == tick {
                last.entities.push(entity);
                return;
            }
        }

        if self.history.len() >= self.max_ticks {
            self.history.pop_front();
        }
        self.history.push_back(TickSnapshot {
            tick,
            timestamp_ms,
            entities: vec![entity],
        });
    }

    pub fn find_snapshot(&self, shoot_tick: u64) -> Option<&TickSnapshot> {
        let count = self.history.len();
        if count == 0 {
            return None;
        }

        let (mut lo, mut hi) = (0usize, count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.history[mid].tick < shoot_tick {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo < count && self.history[lo].tick == shoot_tick {
            return Some(&self.history[lo]);
        }

        let oldest = self.history.front().unwrap().tick;
        if shoot_tick < oldest.saturating_sub(60) {
            return None;
        }

        let (prev, next) = if lo == 0 {
            (None, Some(&self.history[0]))
        } else if lo == count {
            (Some(&self.history[count - 1]), None)
        } else {
            (Some(&self.history[lo - 1]), Some(&self.history[lo]))
        };
        match (prev, next) {
            (Some(p), Some(n)) => {
                let dp = (p.tick as i64 - shoot_tick as i64).abs();
                let dn = (n.tick as i64 - shoot_tick as i64).abs();
                Some(if dp <= dn { p } else { n })
            }
            (Some(p), None) => Some(p),
            (None, Some(n)) => Some(n),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LagCompensatedHit {
    pub point: Vec3,
    pub distance: f64,
    pub entity_id: u32,
    pub kind: u32,
    pub collider_id: u32,
    pub hit_type: u32,
}
