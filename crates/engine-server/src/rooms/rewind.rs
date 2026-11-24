//! Generic per-entity rewind history.
//!
//! Physics lag compensation is one consumer, but the same mechanism serves
//! hit validation, cooldown checks and any game-specific "what did this look
//! like N ms ago?" query. The history stores an opaque f64 vector per sample;
//! the guest names the fields, the host only stores and looks up.

use std::collections::VecDeque;

pub const MAX_REWIND_FRAMES: usize = 256;
pub const MAX_REWIND_FIELDS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct RewindFrame {
    pub tick: i64,
    /// Authoritative time of the sample. Legacy `record` keeps `tick` as the
    /// clock; `record_time` stores millisecond timestamps for interpolation.
    pub time_ms: f64,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct RewindTrack {
    width: usize,
    frames: VecDeque<RewindFrame>,
}

impl RewindTrack {
    pub fn record(&mut self, tick: i64, values: Vec<f64>) -> Result<(), String> {
        self.record_time(tick, tick as f64, values)
    }

    /// Record a sample with an explicit millisecond timestamp. Time queries
    /// interpolate linearly between the two surrounding samples.
    pub fn record_time(&mut self, tick: i64, time_ms: f64, values: Vec<f64>) -> Result<(), String> {
        if values.is_empty() || values.len() > MAX_REWIND_FIELDS {
            return Err(format!(
                "rewind width {} outside 1..={MAX_REWIND_FIELDS}",
                values.len()
            ));
        }
        if tick < 0 {
            return Err("rewind tick must be >= 0".into());
        }
        if !time_ms.is_finite() {
            return Err("rewind time must be finite".into());
        }
        if self.width == 0 {
            self.width = values.len();
        } else if self.width != values.len() {
            return Err(format!(
                "rewind width changed from {} to {} for one entity",
                self.width,
                values.len()
            ));
        }

        let at = self.frames.partition_point(|frame| frame.tick < tick);
        if at < self.frames.len() && self.frames[at].tick == tick {
            self.frames[at].time_ms = time_ms;
            self.frames[at].values = values;
            return Ok(());
        }
        self.frames.insert(
            at,
            RewindFrame {
                tick,
                time_ms,
                values,
            },
        );
        while self.frames.len() > MAX_REWIND_FRAMES {
            self.frames.pop_front();
        }
        Ok(())
    }

    /// Step lookup: the newest sample at or before `tick`.
    pub fn at(&self, tick: i64) -> Option<&[f64]> {
        self.frames
            .iter()
            .rev()
            .find(|frame| frame.tick <= tick)
            .map(|frame| frame.values.as_slice())
    }

    /// Time lookup with linear interpolation between surrounding samples.
    /// Clamps to the oldest/newest sample when `time_ms` is outside history.
    pub fn at_time(&self, time_ms: f64) -> Option<Vec<f64>> {
        let first = self.frames.front()?;
        let last = self.frames.back()?;
        if time_ms <= first.time_ms {
            return Some(first.values.clone());
        }
        if time_ms >= last.time_ms {
            return Some(last.values.clone());
        }
        let mut prev: Option<&RewindFrame> = None;
        for frame in &self.frames {
            if frame.time_ms <= time_ms {
                prev = Some(frame);
            } else {
                let Some(a) = prev else {
                    return Some(frame.values.clone());
                };
                let span = frame.time_ms - a.time_ms;
                if span <= f64::EPSILON {
                    return Some(frame.values.clone());
                }
                let alpha = ((time_ms - a.time_ms) / span).clamp(0.0, 1.0);
                let values = a
                    .values
                    .iter()
                    .zip(frame.values.iter())
                    .map(|(left, right)| left + (right - left) * alpha)
                    .collect();
                return Some(values);
            }
        }
        prev.map(|frame| frame.values.clone())
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn samples(&self) -> usize {
        self.frames.len()
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.width = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_ordered_and_same_tick_replaces() {
        let mut track = RewindTrack::default();
        track.record(10, vec![1.0, 2.0]).unwrap();
        track.record(5, vec![0.0, 0.0]).unwrap();
        track.record(10, vec![3.0, 4.0]).unwrap();
        assert_eq!(track.at(10), Some([3.0, 4.0].as_slice()));
        assert_eq!(track.at(7), Some([0.0, 0.0].as_slice()));
        assert_eq!(track.samples(), 2);
    }

    #[test]
    fn history_is_bounded_and_width_is_stable() {
        let mut track = RewindTrack::default();
        for tick in 0..(MAX_REWIND_FRAMES as i64 + 20) {
            track.record(tick, vec![tick as f64]).unwrap();
        }
        assert_eq!(track.samples(), MAX_REWIND_FRAMES);
        assert_eq!(track.at(0), None);
        assert!(track.record(1, vec![1.0, 2.0]).is_err());
    }

    #[test]
    fn time_queries_interpolate_and_clamp() {
        let mut track = RewindTrack::default();
        track.record_time(10, 100.0, vec![1.0, 10.0]).unwrap();
        track.record_time(11, 150.0, vec![3.0, 30.0]).unwrap();
        assert_eq!(track.at_time(90.0), Some(vec![1.0, 10.0]));
        assert_eq!(track.at_time(125.0), Some(vec![2.0, 20.0]));
        assert_eq!(track.at_time(900.0), Some(vec![3.0, 30.0]));
        assert_eq!(track.at(10), Some([1.0, 10.0].as_slice()));
    }

    #[test]
    fn an_empty_or_wide_frame_is_refused() {
        let mut track = RewindTrack::default();
        assert!(track.record(1, Vec::new()).is_err());
        assert!(track.record(1, vec![0.0; MAX_REWIND_FIELDS + 1]).is_err());
    }
}
