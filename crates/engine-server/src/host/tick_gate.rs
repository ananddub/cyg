use std::time::{Duration, Instant};

use crate::host::*;
use crate::rooms::CloseReason;

impl TickRunner {
    pub(crate) fn run(&mut self, ctx: &mut TickCtx) {
        let tick_interval = ctx.tick_interval;
        let dt = tick_interval.as_secs_f64();

        // Interval boundary: budget the catch-up, refresh dead-room scan.
        // Reap stays after the phase runs below so the order matches the
        // historical scan → tick → cleanup → reap → flush pass.
        let elapsed = self.last_tick.elapsed();
        if elapsed >= tick_interval {
            let mut ticks_to_run = (elapsed.as_secs_f64() / tick_interval.as_secs_f64()) as u32;
            if ticks_to_run > ctx.cfg.watchdog.max_ticks_per_frame {
                ticks_to_run = ctx.cfg.watchdog.max_ticks_per_frame;
                self.last_tick = Instant::now();
            } else {
                self.last_tick += tick_interval * ticks_to_run;
            }
            self.interval_pending = ticks_to_run;
            self.tick_counter += ticks_to_run as u64;
            self.tick_stat(ctx, dt);
            // Room snapshot ONCE per interval — the fan-out reuses it for
            // every catch-up pass and phase slice; a room created mid-interval
            // joins at the next boundary.
            self.room_names.clear();
            self.room_names.extend(
                ctx.server
                    .rooms
                    .iter()
                    .map(|(name, room)| (name.clone(), room.tick_phase)),
            );
            let reap_now = Instant::now();
            self.tick_scan_dead(ctx, reap_now);
        }

        let groups = ctx.cfg.tick_stagger.max(1);
        let run_t0 = Instant::now();
        if groups <= 1 {
            if self.interval_pending > 0 {
                let pending = self.interval_pending;
                self.interval_pending = 0;
                self.run_phase(ctx, None, pending, dt, run_t0);
            }
        } else {
            // Staggered: the loop wakes once per slot (see wait_events); every
            // slot boundary fires the next phase's rooms. Each phase runs once
            // per interval, so every room still ticks at the full tick rate —
            // only the phase within the interval shifts. `interval_pending`
            // stays armed until the next interval boundary overwrites it:
            // zeroing it after the first slot would leave the remaining
            // phases nothing to run, dropping each room to 1/groups of the
            // tick rate.
            for phase in self.stagger_phases_due(tick_interval, groups) {
                if self.interval_pending > 0 {
                    self.run_phase(ctx, Some(phase), self.interval_pending, dt, run_t0);
                }
            }
        }

        ctx.udp.cleanup(ctx.server, ctx.cfg);
        self.tick_reap_dead(ctx);

        // No flush here — Engine::step flushes ONCE at the end. The old
        // second flush pass per step doubled writev syscalls on quiet frames,
        // and its `expect` turned one transient socket error into a full
        // process abort (panic = "abort" release builds).
    }

    /// Advance the stagger clock by the slots that elapsed since the last
    /// wake, returning the phase each due slot fires. Stalling past a whole
    /// interval (busy frame, debugger) resyncs instead of replaying a backlog
    /// of empty slots.
    fn stagger_phases_due(&mut self, tick_interval: Duration, groups: u32) -> Vec<u32> {
        let slot = tick_interval / groups;
        let mut slots = self.slot_base.elapsed().as_nanos() / slot.as_nanos().max(1);
        if slots > groups as u128 {
            self.slot_base = Instant::now();
            slots = 1;
        }
        let mut phases = Vec::with_capacity(slots as usize);
        for _ in 0..slots {
            phases.push(self.next_phase);
            self.next_phase = (self.next_phase + 1) % groups;
            self.slot_base += slot;
        }
        phases
    }

    pub(crate) fn tick_stat(&mut self, ctx: &TickCtx, dt: f64) {
        let tick_interval = ctx.tick_interval;
        if self.tick_counter >= 300 && self.tick_stat_start.elapsed() >= Duration::from_secs(5) {
            let stat_dur = self.tick_stat_start.elapsed().as_secs_f64();
            let measured_hz = (self.tick_counter as f64) / stat_dur;
            tracing::debug!(
                "[server] ⏱️ Steady Fixed Tickrate: {:.2} Hz (target: {} Hz, dt: {:.2}ms, 300 samples in {:.2}s)",
                measured_hz,
                live_tick_rate(tick_interval),
                dt * 1000.0,
                stat_dur
            );
            self.tick_counter = 0;
            self.tick_stat_start = Instant::now();
        }
    }

    pub(crate) fn tick_scan_dead(&mut self, ctx: &mut TickCtx, reap_now: Instant) {
        self.dead_rooms.clear();
        for (name, room) in ctx.server.rooms.iter_mut() {
            let live = room.sessions.len() as u32;
            match self.match_peaks.get_mut(name.as_ref()) {
                Some(peak) => *peak = (*peak).max(live),
                None => {
                    self.match_peaks.insert(name.to_string(), live);
                }
            }
            let reapable = room_is_reapable(ctx.cfg, name, ctx.udp.udp_peers.len());
            if let Some(reason) = room.close_reason(&ctx.cfg.lifecycle, reapable, reap_now) {
                self.dead_rooms.push((name.to_string(), reason));
            }
        }
    }

    pub(crate) fn tick_reap_dead(&mut self, ctx: &mut TickCtx) {
        for (name, reason) in self.dead_rooms.drain(..) {
            if let Some(mut room) = ctx.server.rooms.remove(name.as_str()) {
                if reason == CloseReason::Duration && !room.is_hibernating() {
                    close_room_sessions(
                        ctx.transport,
                        &room,
                        engine_net::close_code::CONSENTED,
                        "room reached its time limit",
                    );
                }
                if let Some(snapshot) = room.export_snapshot() {
                    report_snapshot(ctx.bus, &room, &name, &snapshot);
                    ctx.server.save_snapshot(&name, snapshot);
                }
                let peak = self
                    .match_peaks
                    .remove(&name)
                    .unwrap_or(room.sessions.len() as u32);
                // Read what the round reported before disposing: `on_dispose`
                // is the guest's chance to clear its own state, so a match
                // result read afterwards would be read from a cleared room.
                room.collect_match_result();
                report_match(ctx.bus, ctx.game_id, &room, &name, reason.label(), peak);
                report_ownership(
                    ctx.bus,
                    ctx.node_id,
                    ctx.game_id,
                    &name,
                    &room,
                    engine_bus::RoomPhase::Closed,
                );
                room.dispose();
                tracing::info!(
                    "[server] room '{name}' disposed and state saved ({})",
                    reason.label()
                );
            }
        }
    }
}

#[cfg(test)]
mod stagger_tests {
    use super::*;

    fn runner(slot_base: Instant) -> TickRunner {
        TickRunner {
            session_scratch: Vec::new(),
            room_names: Vec::new(),
            delta_buf: Vec::new(),
            view_buf: Vec::new(),
            delta_body: Vec::new(),
            filtered_buf: Vec::new(),
            udp_batch: Vec::new(),
            schema_aoi: None,
            dead_rooms: Vec::new(),
            last_tick: slot_base,
            tick_counter: 0,
            tick_stat_start: slot_base,
            window_ticks: 0,
            stage_sim_us_sum: 0,
            stage_guest_us_sum: 0,
            match_peaks: HashMap::new(),
            next_phase: 0,
            slot_base,
            interval_pending: 0,
            schema_full: 0..0,
            schema_patch: 0..0,
            wrap_full_packed: Vec::new(),
            wrap_patch_packed: Vec::new(),
            packed_full_body: Vec::new(),
            packed_patch_body: Vec::new(),
            packed_view_buf: Vec::new(),
            packed_lane: None,
            packed_ready: false,
        }
    }

    #[test]
    fn every_slot_fires_the_next_phase_across_a_full_interval() {
        // 3 groups, 30 ms interval → one 10 ms slot per phase per interval.
        let mut r = runner(Instant::now());
        r.slot_base -= Duration::from_millis(35); // all 3 slots are due
        r.interval_pending = 2;
        assert_eq!(r.stagger_phases_due(Duration::from_millis(30), 3), [0, 1, 2]);
        assert_eq!(r.next_phase, 0);
        // The pending budget is consumed by run_phase per slot, not zeroed
        // here — a room's phase must still see it on its slot.
        assert_eq!(r.interval_pending, 2);
    }

    #[test]
    fn stalled_wake_resyncs_instead_of_replaying_empty_slots() {
        let mut r = runner(Instant::now());
        r.slot_base -= Duration::from_millis(500); // far past one interval
        assert_eq!(r.stagger_phases_due(Duration::from_millis(30), 3), [0]);
        assert_eq!(r.next_phase, 1);
    }
}
