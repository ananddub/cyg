use std::time::Instant;

use crate::host::*;

impl TickRunner {
    /// Tick rooms for `ticks_to_run` catch-up passes. `phase = None` runs
    /// every room (staggering off); `Some(p)` runs only the rooms whose
    /// creation-time slot is `p` — the room-name snapshot was collected once
    /// at the interval boundary, so this loop allocates nothing.
    pub(crate) fn run_phase(
        &mut self,
        ctx: &mut TickCtx,
        phase: Option<u32>,
        ticks_to_run: u32,
        dt: f64,
        run_t0: Instant,
    ) {
        for _tick_idx in 0..ticks_to_run {
            let mut names = std::mem::take(&mut self.room_names);
            for (name, room_phase) in &names {
                if let Some(p) = phase {
                    if *room_phase != p {
                        continue;
                    }
                }
                self.tick_one_room(ctx, dt, name);
            }
            self.room_names = names;
            self.tick_after_batch(ctx, run_t0, ticks_to_run, run_t0);
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

    pub(crate) fn tick_one_room(&mut self, ctx: &mut TickCtx, dt: f64, name: &str) {
        let history = EventHistory::new(ctx.bus, ctx.game_cfg);
        let room = match ctx.server.rooms.get_mut(name) {
            Some(room) => room,
            None => return,
        };
        room.note_state();
        let raw_peers_attached = ctx.udp.peers_in_room(name) > 0;
        if room.lifecycle.phase != engine_bus::RoomPhase::Active && !raw_peers_attached {
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
        // Idle synthesis (hold-last, gated on liveness) runs before the tick
        // so empty-input seats still feed the guest this tick. RTC/UDP lanes
        // report liveness directly; a pure-WS seat has no lane probe, so it
        // stays "alive" until the disconnect path (or pending-leave expiry)
        // removes the seat. The ECS backend keeps per-entity last input
        // flags in `SessionInputs` — it already holds, so synthesis stays a
        // wasm-backend concern.
        let transport = &*ctx.transport;
        let udp_peers = &ctx.udp.udp_peers;
        room.synthesize_idle_inputs(|session| {
            transport.is_rtc_connected(session)
                || udp_peers.values().any(|(_, sess, _)| sess == session)
        });
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
        // Read the delta on EVERY catch-up tick: keeping the `is_last_tick`
        // gate discarded all but the last tick's changes, so a burst of
        // catch-up frames shipped only its final state.
        if len > 0 {
            if let Some(ref mut wasm) = room.wasm {
                if let Err(e) = wasm.read_memory_into(ptr as u32, len as u32, &mut self.delta_buf) {
                    tracing::error!(
                        "[server] ⚠️ failed to read delta memory in room '{name}': {e}"
                    );
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
    fn rooms_get_round_robin_tick_phases_at_creation() {
        let mut server = GameServer::new_with_features(
            None,
            16,
            engine_physics::PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
            Default::default(),
        );
        for (i, name) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            let room = server.get_or_create(name, b"").unwrap();
            assert_eq!(
                room.tick_phase,
                (i as u32) % u32::MAX,
                "creation order hands out consecutive slots"
            );
        }
        assert_eq!(server.next_room_phase, 5);
    }
}
