use std::time::{Duration, Instant};

use crate::host::*;
use crate::rooms::CloseReason;

impl TickRunner {
    pub(crate) fn run(&mut self, ctx: &mut TickCtx) {
        let tick_interval = ctx.tick_interval;
        let elapsed = self.last_tick.elapsed();
        if elapsed < tick_interval {
            return;
        }
        let __gate_t0 = Instant::now();
        let mut ticks_to_run = (elapsed.as_secs_f64() / tick_interval.as_secs_f64()) as u32;
        if ticks_to_run > ctx.cfg.watchdog.max_ticks_per_frame {
            ticks_to_run = ctx.cfg.watchdog.max_ticks_per_frame;
            self.last_tick = Instant::now();
        } else {
            self.last_tick += tick_interval * ticks_to_run;
        }
        let dt = tick_interval.as_secs_f64();

        self.tick_counter += ticks_to_run as u64;
        self.tick_stat(ctx, dt);

        let tick_start = Instant::now();
        let reap_now = Instant::now();

        self.tick_scan_dead(ctx, reap_now);
        self.tick_all_rooms(ctx, __gate_t0, ticks_to_run, dt, tick_start);
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
            match self.match_peaks.get_mut(name.as_str()) {
                Some(peak) => *peak = (*peak).max(live),
                None => {
                    self.match_peaks.insert(name.clone(), live);
                }
            }
            let reapable = room_is_reapable(ctx.cfg, name, ctx.udp.udp_peers.len());
            if let Some(reason) = room.close_reason(&ctx.cfg.lifecycle, reapable, reap_now) {
                self.dead_rooms.push((name.clone(), reason));
            }
        }
    }

    pub(crate) fn tick_reap_dead(&mut self, ctx: &mut TickCtx) {
        for (name, reason) in self.dead_rooms.drain(..) {
            if let Some(mut room) = ctx.server.rooms.remove(&name) {
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
                tracing::info!("[server] room '{name}' disposed and state saved ({})", reason.label());
            }
        }
    }
}
