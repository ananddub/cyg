//! # Lag Compensation & Rewind History
//!
//! Maintains historical entity snapshots in a circular ringbuffer over a window
//! (e.g. 500ms). When a player shoots or performs an action with latency, the server
//! rewinds target positions to the exact millisecond the shooting player saw on their screen.
//!
//! Performance notes (hot path):
//! - Structure-of-arrays storage (`ticks` / `times` / `data` columns) keeps each
//!   lookup touching only the memory it needs, like Colyseus' `Float64Array` columns.
//! - O(1) sample lookup when ticks are contiguous (the per-tick recording case):
//!   the bounding pair is computed by index arithmetic instead of a backward scan.
//! - `Interpolation::Step` holds the pre-`time` value for discrete motion
//!   (teleports), where a lerp would smear across the jump.

use std::collections::HashMap;

/// Standard 3D transform snapshot for game entities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformSnapshot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    pub hit_radius: f32,
}

impl TransformSnapshot {
    pub fn new(x: f32, y: f32, z: f32, hit_radius: f32) -> Self {
        Self {
            x,
            y,
            z,
            rotation_y: 0.0,
            hit_radius,
        }
    }

    /// Linear interpolation between two snapshots.
    #[inline]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
            rotation_y: self.rotation_y + (other.rotation_y - self.rotation_y) * t,
            hit_radius: self.hit_radius + (other.hit_radius - self.hit_radius) * t,
        }
    }

    /// Check if a ray or point hits this entity at this snapshot position.
    #[inline]
    pub fn is_point_hit(&self, px: f32, py: f32, pz: f32) -> bool {
        let dx = self.x - px;
        let dy = self.y - py;
        let dz = self.z - pz;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= (self.hit_radius * self.hit_radius)
    }
}

/// How a value is reconstructed between two recorded samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// Blend between the two bounding samples (continuous motion).
    #[default]
    Linear,
    /// Hold the value of the sample *before* `time` (discrete motion, teleports).
    Step,
}

/// A recorded sample in time.
#[derive(Debug, Clone, Copy)]
pub struct HistorySample<T> {
    pub tick: u64,
    pub time_ms: f64,
    pub data: T,
}

/// The two bounding samples around a target time, plus the blend factor.
struct Bounding<T> {
    prev: HistorySample<T>,
    next: HistorySample<T>,
    t: f32,
}

/// Circular history ringbuffer for a single entity, stored as parallel columns.
#[derive(Debug, Clone)]
pub struct EntityHistory<T> {
    ticks: Vec<u64>,
    times: Vec<f64>,
    data: Vec<T>,
    head: usize,
    count: usize,
    capacity: usize,
    interp: Interpolation,
}

impl<T: Copy> EntityHistory<T> {
    pub fn new(capacity: usize) -> Self {
        Self::with_interpolation(capacity, Interpolation::Linear)
    }

    pub fn with_interpolation(capacity: usize, interp: Interpolation) -> Self {
        let cap = capacity.max(2);
        Self {
            ticks: Vec::with_capacity(cap),
            times: Vec::with_capacity(cap),
            data: Vec::with_capacity(cap),
            head: 0,
            count: 0,
            capacity: cap,
            interp,
        }
    }

    /// Record a state sample at a given tick and time.
    #[inline]
    pub fn record(&mut self, tick: u64, time_ms: f64, data: T) {
        if self.count < self.capacity {
            self.ticks.push(tick);
            self.times.push(time_ms);
            self.data.push(data);
            self.head = (self.head + 1) % self.capacity;
            self.count += 1;
        } else {
            self.ticks[self.head] = tick;
            self.times[self.head] = time_ms;
            self.data[self.head] = data;
            self.head = (self.head + 1) % self.capacity;
        }
    }

    /// Number of retained samples.
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no samples are retained.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    fn sample_at(&self, logical: usize) -> HistorySample<T> {
        let phys = (self.head + self.capacity - self.count + logical) % self.capacity;
        HistorySample {
            tick: self.ticks[phys],
            time_ms: self.times[phys],
            data: self.data[phys],
        }
    }

    /// Find the two bounding samples around `target_time_ms`, clamped to the
    /// retained range. O(1) for contiguous ticks, O(n) fallback otherwise.
    pub fn find_samples(&self, target_time_ms: f64) -> Option<(T, T, f32)> {
        if self.count == 0 {
            return None;
        }

        let newest = self.sample_at(self.count - 1);
        let oldest = self.sample_at(0);

        // Clamped to oldest.
        if target_time_ms <= oldest.time_ms {
            return Some((oldest.data, oldest.data, 0.0));
        }

        // Clamped to newest.
        if target_time_ms >= newest.time_ms {
            return Some((newest.data, newest.data, 1.0));
        }

        let b = self.find_bounding(target_time_ms, oldest, newest)?;
        Some((b.prev.data, b.next.data, b.t))
    }

    #[inline]
    fn find_bounding(
        &self,
        target_time_ms: f64,
        oldest: HistorySample<T>,
        newest: HistorySample<T>,
    ) -> Option<Bounding<T>> {
        // Fast path: contiguous ticks (recorded once per tick) let us jump
        // straight to the bounding pair by index arithmetic. The estimate can
        // be a few samples off when local gaps differ from the average rate,
        // so locally walk until prev.time <= target <= next.time holds.
        let tick_span = newest.tick - oldest.tick;
        if tick_span as usize == self.count - 1 && tick_span > 0 {
            let ms_per_tick = (newest.time_ms - oldest.time_ms) / tick_span as f64;
            if ms_per_tick > 0.0 {
                let offset = ((target_time_ms - oldest.time_ms) / ms_per_tick).floor();
                let last = self.count - 2;
                let mut i = (offset as usize).min(last);
                // Enforce the bracket invariant.
                while i > 0 && self.sample_at(i).time_ms > target_time_ms {
                    i -= 1;
                }
                while i < last && self.sample_at(i + 1).time_ms < target_time_ms {
                    i += 1;
                }
                let prev = self.sample_at(i);
                let next = self.sample_at(i + 1);
                if target_time_ms >= prev.time_ms && target_time_ms <= next.time_ms {
                    let t = if next.time_ms > prev.time_ms {
                        ((target_time_ms - prev.time_ms) / (next.time_ms - prev.time_ms)) as f32
                    } else {
                        0.0
                    };
                    return Some(Bounding { prev, next, t });
                }
            }
        }

        // Fallback: backward scan from newest (handles irregular cadence).
        for i in (0..self.count.saturating_sub(1)).rev() {
            let prev = self.sample_at(i);
            let next = self.sample_at(i + 1);
            if target_time_ms >= prev.time_ms && target_time_ms <= next.time_ms {
                let t = if next.time_ms > prev.time_ms {
                    ((target_time_ms - prev.time_ms) / (next.time_ms - prev.time_ms)) as f32
                } else {
                    0.0
                };
                return Some(Bounding { prev, next, t });
            }
        }

        Some(Bounding {
            prev: newest,
            next: newest,
            t: 1.0,
        })
    }
}

/// Multi-entity Lag Compensation manager for a game room.
#[derive(Debug, Clone)]
pub struct RewindManager {
    /// History capacity per entity (e.g. 60 samples at 60Hz = 1000ms history).
    capacity: usize,
    interp: Interpolation,
    histories: HashMap<u64, EntityHistory<TransformSnapshot>>,
}

impl RewindManager {
    /// Create a new manager with the specified rewind window (in milliseconds)
    /// and tick rate (e.g. 500ms window at 60Hz = 30 samples).
    pub fn new(max_rewind_ms: u32, tick_rate: u32) -> Self {
        Self::new_with_interpolation(max_rewind_ms, tick_rate, Interpolation::Linear)
    }

    /// Create a manager that reconstructs with the given interpolation mode.
    /// Use [`Interpolation::Step`] for games with teleporting entities.
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

    /// Record the transform of an entity at the current tick and time.
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

    /// Rewind a single entity to its exact position at `target_time_ms`.
    pub fn rewind_entity(&self, entity_id: u64, target_time_ms: f64) -> Option<TransformSnapshot> {
        let history = self.histories.get(&entity_id)?;
        let (prev, next, t) = history.find_samples(target_time_ms)?;
        Some(match history.interp {
            Interpolation::Step => prev,
            Interpolation::Linear => prev.lerp(&next, t),
        })
    }

    /// Rewind all entities in the room to `target_time_ms` for hit-registration tests.
    pub fn rewind_all(&self, target_time_ms: f64) -> Vec<(u64, TransformSnapshot)> {
        let mut out = Vec::with_capacity(self.histories.len());
        self.rewind_all_into(target_time_ms, &mut out);
        out
    }

    /// Allocation-free variant of [`Self::rewind_all`]: reuse a caller-owned buffer.
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

    /// Remove an entity when it leaves or is destroyed.
    pub fn remove_entity(&mut self, entity_id: u64) {
        self.histories.remove(&entity_id);
    }
}
