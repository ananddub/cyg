use std::time::Instant;

use crate::host::*;

impl TickRunner {
    pub(crate) fn tick_all_rooms(
        &mut self,
        ctx: &mut TickCtx,
        __gate_t0: Instant,
        ticks_to_run: u32,
        dt: f64,
        tick_start: Instant,
    ) {
        for tick_idx in 0..ticks_to_run {
            let is_last_tick = tick_idx + 1 == ticks_to_run;
            let names: Vec<String> = ctx.server.rooms.keys().cloned().collect();
            for name in names {
                self.tick_one_room(ctx, is_last_tick, dt, &name);
            }
            self.tick_after_batch(ctx, __gate_t0, ticks_to_run, tick_start);
        }
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
                flush_room_outbox(room, ctx.transport, name, &history);
                return;
            }
            Err(e) => {
                tracing::warn!("[server] ⚠️ guest trap during on_tick in room '{name}': {e}");
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
