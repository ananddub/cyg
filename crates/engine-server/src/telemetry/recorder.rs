//! The sim-thread recorder: latency rings, tick durations, overrun counting.

use std::time::{Duration, Instant};

use engine_bus::TICK_HISTOGRAM_BUCKETS;
use engine_dashboard::histogram_add;

use engine_bus::clock::tick_period;
use super::stats::{RING, ring_push};

pub struct Recorder {
    pub(super) input_lat_ns: [u64; RING],
    pub(super) tick_us: [u64; RING],
    pub(super) n_input: usize,
    pub(super) n_tick: usize,
    /// Tick durations of the current window, added by `note_tick` and rolled
    /// by `maybe_publish` exactly like the rings.
    pub(super) tick_hist: [u32; TICK_HISTOGRAM_BUCKETS],
    /// Armed when the session set empties, cleared the moment it is occupied
    /// again; the source of `empty_for_sec`.
    pub(super) empty_since: Option<Instant>,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub inputs_dropped: u64,
    pub tick_overruns: u64,
    pub(super) last_publish: Instant,
    /// Nominal tick rate, refreshed by `maybe_publish`; used only to classify
    /// a tick duration as an overrun against `1/tick_hz` of work.
    pub(super) tick_hz: f64,
    pub(super) publish_seq: u64,
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder {
            input_lat_ns: [0; RING],
            tick_us: [0; RING],
            n_input: 0,
            n_tick: 0,
            tick_hist: [0; TICK_HISTOGRAM_BUCKETS],
            empty_since: None,
            deltas_sent: 0,
            deltas_dropped_full: 0,
            inputs_dropped: 0,
            tick_overruns: 0,
            last_publish: Instant::now(),
            tick_hz: 128.0,
            publish_seq: 0,
        }
    }

    /// kernel→sim latency of one input, nanoseconds. Saturating: a stale
    /// arrival stamp must never panic the sim thread.
    pub fn note_input_latency(&mut self, arrival_ns: u64, now_ns: u64) {
        ring_push(&mut self.input_lat_ns, &mut self.n_input, now_ns.saturating_sub(arrival_ns));
    }

    /// wall duration of one `wasm.on_tick`.
    pub fn note_tick(&mut self, dur: Duration) {
        let us = dur.as_micros().min(u64::MAX as u128) as u64;
        ring_push(&mut self.tick_us, &mut self.n_tick, us);
        histogram_add(&mut self.tick_hist, us);
        if dur > tick_period(self.tick_hz) {
            self.tick_overruns += 1;
        }
    }

    /// `true` when the next `maybe_publish` will actually roll a window, so a
    /// caller can skip building session rows in the middle of a tick.
    pub fn publish_due(&self) -> bool {
        self.last_publish.elapsed() >= Duration::from_secs(1)
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder::new()
    }
}
