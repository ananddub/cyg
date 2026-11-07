use std::time::Instant;

use crate::host::*;

impl TickRunner {
    /// Tick every room belonging to `phase` for `ticks_to_run` catch-up
    /// passes. With staggering off, all rooms share phase 0 and this is the
    /// whole tick; with staggering on, each phase's slice runs separately.
    pub(crate) fn run_phase(
        &mut self,
        ctx: &mut TickCtx,
        phase: u32,
        ticks_to_run: u32,
        dt: f64,
        run_t0: Instant,
    ) {
        for tick_idx in 0..ticks_to_run {
            let is_last_tick = tick_idx + 1 == ticks_to_run;
            let names: Vec<String> = ctx.server.rooms.keys().cloned().collect();
            for name in names {
                if self.phase_of(&name, ctx.cfg.tick_stagger.max(1)) != phase {
                    continue;
                }
                self.tick_one_room(ctx, is_last_tick, dt, &name);
            }
            self.tick_after_batch(ctx, run_t0, ticks_to_run, run_t0);
        }
    }

    fn phase_of(&mut self, name: &str, groups: u32) -> u32 {
        if let Some(phase) = self.room_phase.get(name) {
            return *phase;
        }
        let phase = self.phase_ctr % groups;
        self.phase_ctr = self.phase_ctr.wrapping_add(1);
        self.room_phase.insert(name.to_string(), phase);
        phase
    }

    pub(crate) fn tick_after_batch(
        &mut self,
        ctx: &mut TickCtx,
        __gate_t0: Instant,
        ticks_to_run: u32,
        tick_start: Instant,
    ) {
        ctx.recorder.note_tick(tick_start.elapsed());
        self.stage_sim_us_sum += __gate_t0.elapsed().as_micros() as u64
            / ticks_to_run.max(1) as u64
            * ticks_to_run as u64;
        self.window_ticks += ticks_to_run as u64;
    }

    pub(crate) fn tick_one_room(&mut self, ctx: &mut TickCtx, is_last_tick: bool, dt: f64, name: &str) {
        let history = EventHistory::new(ctx.bus, ctx.game_cfg);
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        room.note_state();
        let raw_peers_attached = name == ctx.cfg.name && !ctx.udp.udp_peers.is_empty();
        if room.state != engine_bus::RoomPhase::Active && !raw_peers_attached {
            return;
        }
        if room.expire_pending_leaves(Instant::now()) {
            flush_room_outbox(room, ctx.transport, name, &history);
        }
        if room.is_ecs() {
            let guest_t0 = Instant::now();
            room.tick_ecs(dt);
            self.stage_guest_us_sum += guest_t0.elapsed().as_micros() as u64;
            flush_room_outbox(room, ctx.transport, name, &history);
            return;
        }
        // Tell the guest up front whether this tick's commit should seal:
        // due commits accumulate every change since the last send, so patches
        // stay incremental even when patch_rate < tick_rate.
        let patch_due = room.next_patch_due(Instant::now(), ctx.cfg.patch_rate_ms());
        room.signal_patch_due(patch_due);
        let guest_t0 = Instant::now();
        let ticked = room.tick(dt, ctx.cfg.rollback.enabled);
        self.stage_guest_us_sum += guest_t0.elapsed().as_micros() as u64;
        let packed = match ticked {
            Ok(p) => p,
            Err(e) if e.is_interrupt() => {
                ctx.recorder.tick_overruns += 1;
                tracing::warn!(
                    "[server] ⛔ watchdog cut off a guest call in room '{name}' after {}ms; the room stays up and the next call re-arms, but its state may be torn",
                    ctx.cfg.watchdog.tick_budget_ms
                );
                room.guest_on_uncaught_exception("the watchdog cut off onTick", "onTick");
                flush_room_outbox(room, ctx.transport, name, &history);
                return;
            }
            Err(e) => {
                tracing::warn!("[server] ⚠️ guest trap during on_tick in room '{name}': {e}");
                room.guest_on_uncaught_exception(&e.to_string(), "onTick");
                flush_room_outbox(room, ctx.transport, name, &history);
                return;
            }
        };
        let ptr = ((packed >> 32) as u32) as usize;
        let len = (packed as u32) as usize;
        flush_room_outbox(room, ctx.transport, name, &history);
        if len > engine_wasm::MAX_OUTBOUND_DELTA_SIZE {
            tracing::warn!(
                "[server] ⚠️ delta buffer size {len} exceeded max limit {} in room '{name}'",
                engine_wasm::MAX_OUTBOUND_DELTA_SIZE
            );
            return;
        }
        if len > 0 && is_last_tick {
            if let Some(ref mut wasm) = room.wasm {
                if let Err(e) =
                    wasm.read_memory_into(ptr as u32, len as u32, &mut self.delta_buf)
                {
                    tracing::error!("[server] ⚠️ failed to read delta memory in room '{name}': {e}");
                    return;
                }
            }
            self.tick_delta(ctx, name);
        }
    }
}

#[cfg(test)]
mod stagger_phase_tests {
    use super::*;

    #[test]
    fn phases_are_handed_out_round_robin_and_stable_per_room() {
        let mut runner = TickRunner {
            session_scratch: Vec::new(),
            delta_buf: Vec::new(),
            view_buf: Vec::new(),
            frame_buf: Vec::new(),
            delta_body: Vec::new(),
            filtered_buf: Vec::new(),
            udp_batch: Vec::new(),
            schema_aoi: None,
            dead_rooms: Vec::new(),
            last_tick: Instant::now(),
            tick_counter: 0,
            tick_stat_start: Instant::now(),
            window_ticks: 0,
            stage_sim_us_sum: 0,
            stage_guest_us_sum: 0,
            match_peaks: HashMap::new(),
            room_phase: HashMap::new(),
            phase_ctr: 0,
            next_phase: 0,
            slot_base: Instant::now(),
            interval_pending: 0,
        };

        let first = runner.phase_of("room_a", 4);
        let second = runner.phase_of("room_b", 4);
        let third = runner.phase_of("room_c", 4);
        let fourth = runner.phase_of("room_d", 4);
        assert_eq!(
            (first, second, third, fourth),
            (0, 1, 2, 3),
            "rooms spread across every slot instead of piling on phase 0"
        );
        assert_eq!(runner.phase_of("room_a", 4), 0, "a room keeps its phase");
        assert_eq!(runner.phase_of("room_e", 4), 0, "the wheel wraps around");
    }
}
