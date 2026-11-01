use std::time::{Duration, Instant};

use crate::plane::sim::state::Plane;

impl Plane {
    pub(super) fn quiet_frame(&mut self) {
        if self.recorder.publish_due() {
            let window = self.window_ticks.max(1);
            let report = self.report(
                (self.stage_sim_us_sum / window) as u64,
                (self.stage_guest_us_sum / window) as u64,
            );
            self.recorder.maybe_publish(&self.bus, report);
            crate::host::report_ownership(
                &self.bus,
                &self.node_id,
                &self.cfg.room_name,
                &self.cfg.room_name,
                &self.room,
                self.room.phase(),
            );
        }
        if self.sweep_start.elapsed() >= Duration::from_millis(200) {
            self.udp.sweep();
            self.sweep_start = Instant::now();
        }
    }

    pub(super) fn maintenance(&mut self) {
        if self.sweep_start.elapsed() >= Duration::from_millis(200) {
            self.udp.sweep();
            self.sweep_start = Instant::now();
        }
        if self.status_start.elapsed() >= Duration::from_secs(1) {
            let ticks = self.window_ticks;
            self.measured_hz = ticks as f64 / self.status_start.elapsed().as_secs_f64();
            self.window_ticks = 0;
            self.status_start = Instant::now();
            self.last_input_at
                .retain(|peer, _| self.room.sessions.contains_key(peer));
            let report = self.report(
                (self.stage_sim_us_sum / ticks.max(1)) as u64,
                (self.stage_guest_us_sum / ticks.max(1)) as u64,
            );
            if self.recorder.maybe_publish(&self.bus, report) {
                self.stage_sim_us_sum = 0;
                self.stage_guest_us_sum = 0;
            } else {
                self.window_ticks = ticks;
            }
            crate::host::report_ownership(
                &self.bus,
                &self.node_id,
                &self.cfg.room_name,
                &self.cfg.room_name,
                &self.room,
                self.room.phase(),
            );
        }
    }
}
