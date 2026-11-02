use super::types::{Bounding, HistorySample, Interpolation};
#[derive(Debug, Clone)]
pub struct EntityHistory<T> {
    ticks: Vec<u64>,
    times: Vec<f64>,
    data: Vec<T>,
    head: usize,
    count: usize,
    capacity: usize,
    pub(super) interp: Interpolation,

    last_tick: Option<u64>,
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
            last_tick: None,
        }
    }

    #[inline]
    pub fn record(&mut self, tick: u64, time_ms: f64, data: T) {
        if let Some(last) = self.last_tick {
            if tick <= last {
                return;
            }
        }
        self.last_tick = Some(tick);
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

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

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

    #[inline]
    pub fn find_samples(&self, target_time_ms: f64) -> Option<(T, T, f32)> {
        if self.count == 0 {
            return None;
        }

        let newest = self.sample_at(self.count - 1);
        let oldest = self.sample_at(0);
        if target_time_ms <= oldest.time_ms {
            return Some((oldest.data, oldest.data, 0.0));
        }

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

        let tick_span = newest.tick - oldest.tick;
        if tick_span as usize == self.count - 1 && tick_span > 0 {
            let ms_per_tick = (newest.time_ms - oldest.time_ms) / tick_span as f64;
            if ms_per_tick > 0.0 {
                let offset = ((target_time_ms - oldest.time_ms) / ms_per_tick).floor();
                let last = self.count - 2;
                let mut i = (offset as usize).min(last);

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
