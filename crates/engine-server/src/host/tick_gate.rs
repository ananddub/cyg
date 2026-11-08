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
            // only the phase within the interval shifts.
            let slot = tick_interval / groups;
            let mut slots = self.slot_base.elapsed().as_nanos() / slot.as_nanos().max(1);
            if slots > groups as u128 {
                // Stalled past a whole interval (busy frame, debugger):
                // resync instead of replaying a backlog of empty slots.
                self.slot_base = Instant::now();
                slots = 1;
            }
            for _ in 0..slots {
                let phase = self.next_phase;
                self.next_phase = (self.next_phase + 1) % groups;
                self.slot_base += slot;
                if self.interval_pending > 0 {
                    let pending = self.interval_pending;
                    self.run_phase(ctx, Some(phase), pending, dt, run_t0);
                }
            }
            self.interval_pending = 0;
        }

        ctx.udp.cleanup(ctx.server, ctx.cfg);
        self.tick_reap_dead(ctx);

        ctx.transport.flush().expect("flush");
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
