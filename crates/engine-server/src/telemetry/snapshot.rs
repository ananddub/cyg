//! Turning the recorder's windows into a `StatusSnapshot`.

use engine_bus::StatusSnapshot;

use engine_bus::clock::mono_ns;
use super::recorder::Recorder;
use super::stats::{percentile, window};

impl Recorder {
    pub fn snapshot(&self) -> StatusSnapshot {
        let lat = window(&self.input_lat_ns, self.n_input);
        let tick = window(&self.tick_us, self.n_tick);
        StatusSnapshot {
            tick_hz_est: self.tick_hz,
            tick_budget_p50_us: percentile(&tick, 50, 100),
            tick_budget_p99_us: percentile(&tick, 99, 100),
            tick_overruns: self.tick_overruns,
            kernel_to_sim_p50_us: percentile(&lat, 50, 100) / 1_000,
            kernel_to_sim_p99_us: percentile(&lat, 99, 100) / 1_000,
            tick_samples: tick.len() as u32,
            tick_histogram: self.tick_hist,
            players: 0,
            deltas_sent: self.deltas_sent,
            deltas_dropped_full: self.deltas_dropped_full,
            inputs_dropped: self.inputs_dropped,
            seq: self.publish_seq,
            updated_ms: mono_ns() / 1_000_000,
            // The session breakdown, nominal period and idle age are stamped by
            // `maybe_publish` from its `PublishReport`, since only the loop
            // that owns the sockets knows what sits on each transport.
            ..Default::default()
        }
    }
}
