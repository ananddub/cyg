use std::collections::HashMap;

use super::history::EntityHistory;
use super::types::{Interpolation, TransformSnapshot};

#[derive(Debug, Clone)]
pub struct RewindManager {
    capacity: usize,
    interp: Interpolation,
    histories: HashMap<u64, EntityHistory<TransformSnapshot>>,
}

impl RewindManager {
    pub fn new(max_rewind_ms: u32, tick_rate: u32) -> Self {
        Self::new_with_interpolation(max_rewind_ms, tick_rate, Interpolation::Linear)
    }

    pub fn new_with_interpolation(
        max_rewind_ms: u32,
        tick_rate: u32,
        interp: Interpolation,
    ) -> Self {
        let sample_interval_ms = 1000.0 / tick_rate as f64;
        let capacity = ((max_rewind_ms as f64 / sample_interval_ms).ceil() as usize + 4).max(8);
        Self {
            capacity,
            interp,
            histories: HashMap::new(),
        }
    }

    #[inline]
    pub fn record(
        &mut self,
        entity_id: u64,
        tick: u64,
        time_ms: f64,
        transform: TransformSnapshot,
    ) {
        let cap = self.capacity;
        let interp = self.interp;
        let entry = self
            .histories
            .entry(entity_id)
            .or_insert_with(|| EntityHistory::with_interpolation(cap, interp));
        entry.record(tick, time_ms, transform);
    }

    pub fn rewind_entity(&self, entity_id: u64, target_time_ms: f64) -> Option<TransformSnapshot> {
        let history = self.histories.get(&entity_id)?;
        let (prev, next, t) = history.find_samples(target_time_ms)?;
        Some(match history.interp {
            Interpolation::Step => prev,
            Interpolation::Linear => prev.lerp(&next, t),
        })
    }

    pub fn rewind_all(&self, target_time_ms: f64) -> Vec<(u64, TransformSnapshot)> {
        let mut out = Vec::with_capacity(self.histories.len());
        self.rewind_all_into(target_time_ms, &mut out);
        out
    }

    pub fn rewind_all_into(&self, target_time_ms: f64, out: &mut Vec<(u64, TransformSnapshot)>) {
        out.clear();
        out.reserve(self.histories.len());
        for (&id, history) in &self.histories {
            if let Some((prev, next, t)) = history.find_samples(target_time_ms) {
                let snap = match history.interp {
                    Interpolation::Step => prev,
                    Interpolation::Linear => prev.lerp(&next, t),
                };
                out.push((id, snap));
            }
        }
    }

    pub fn remove_entity(&mut self, entity_id: u64) {
        self.histories.remove(&entity_id);
    }

    pub fn clear(&mut self) {
        self.histories.clear();
    }

    pub fn len(&self) -> usize {
        self.histories.len()
    }

    pub fn is_empty(&self) -> bool {
        self.histories.is_empty()
    }
}
