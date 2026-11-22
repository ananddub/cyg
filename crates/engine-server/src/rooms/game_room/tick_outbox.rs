use crate::netcode::{self, deliver_input_now};
use crate::rooms::*;
use engine_physics::PhysicsWorld3D;
use std::time::Duration;

impl GameRoom {
    /// Stage a frame for tick-time batch delivery (input_coalesce). Newest
    /// frame per player wins; the seq gate in `apply_input` already ensures
    /// only newer frames reach here.
    pub(crate) fn stage_input(&mut self, id: i64, payload: Vec<u8>) {
        for (staged_id, staged) in &mut self.input_inbox {
            if *staged_id == id {
                *staged = payload;
                return;
            }
        }
        self.input_inbox.push((id, payload));
    }

    /// Deliver every staged frame — one `on_input` per player — then clear
    /// the inbox. Departed players are dropped instead of delivered.
    pub(crate) fn flush_input_inbox(&mut self) {
        if self.input_inbox.is_empty() {
            return;
        }
        let staged = std::mem::take(&mut self.input_inbox);
        for (id, payload) in staged {
            if !self.sessions.values().any(|&pid| pid == id) {
                continue;
            }
            deliver_input_now(self.wasm.as_mut(), id, &payload);
        }
    }

    /// HOLD-LAST idle policy (Colyseus `defineInput({ idle })` hold-everything
    /// form, gated on liveness): a seated, CONNECTED session that sent no
    /// input since the last tick gets its LAST frame re-delivered, so a held
    /// key keeps moving through a packet gap instead of the entity freezing
    /// mid-air while gravity keeps integrating. `alive` is the transport
    /// liveness check — a dropped connection stops the hold immediately, so
    /// a lost client never walks on for the whole reconnection window.
    ///
    /// A synthesized frame is NOT a consumed input: it goes straight through
    /// the staging area (never through `apply_input`), advancing neither the
    /// input seq nor the reconcile ack (`room.seqs.input`, the delta-tail
    /// `acked_seq`). Schema-lane frames get their own seq bumped past the
    /// guest glue's stale gate for the same reason.
    pub(crate) fn synthesize_idle_inputs(&mut self, alive: impl Fn(&str) -> bool) {
        if !self.idle_input_hold {
            return;
        }
        // Candidates collected first — `stage_input` mutates the inbox we
        // would otherwise be iterating beside.
        let candidates: Vec<(i64, Vec<u8>)> = self
            .sessions
            .iter()
            .filter(|(session, id)| {
                !self.inputs_since_tick.contains(id)
                    && !self.lifecycle.pending_leaves.contains_key(*session)
                    && alive(session)
            })
            .filter_map(|(_session, &id)| self.last_input.get(&id).map(|frame| (id, frame.clone())))
            .collect();
        for (id, mut frame) in candidates {
            if netcode::is_input_app_frame(&frame) {
                netcode::bump_input_app_seq(&mut frame);
            }
            self.stage_input(id, frame);
        }
        self.inputs_since_tick.clear();
    }

    pub(crate) fn tick(
        &mut self,
        dt: f64,
        rollback_enabled: bool,
    ) -> Result<i64, engine_wasm::WasmError> {
        self.current_tick += 1;
        self.clock_ms += dt.max(0.0) * 1000.0;
        self.flush_input_inbox();
        let rate_ms = self.guest_simulation_rate_ms();
        let mut guest_dt = dt;
        if rate_ms > 0 {
            let interval = Duration::from_millis(rate_ms as u64);
            self.sim_accum += Duration::from_secs_f64(dt);
            if self.sim_accum < interval {
                return Ok(0);
            }
            self.sim_accum = Duration::ZERO;
            guest_dt = interval.as_secs_f64();
        } else {
            self.sim_accum = Duration::ZERO;
        }
        if let Some(ref mut wasm) = self.wasm {
            wasm.data_mut().clock_ticks = self.current_tick as i64;
            wasm.data_mut().clock_ms = self.clock_ms;
            let packed = wasm.on_tick(guest_dt)?;
            if rollback_enabled {
                let packed_state = wasm.on_save()?;
                let sptr = ((packed_state >> 32) as u32) as usize;
                let slen = (packed_state as u32) as usize;
                let blob = wasm.read_memory(sptr as u32, slen as u32)?;
                self.rollback_buffer
                    .save_blob(u64::from(self.current_tick), blob);
            }
            Ok(packed)
        } else {
            Ok(0)
        }
    }

    pub fn drain_outbox(&mut self) -> Vec<OutboundMessage> {
        let mut out = match self.wasm.as_mut() {
            Some(wasm) => std::mem::take(&mut wasm.data_mut().outbox),
            None => Vec::new(),
        };
        if !self.ecs_outbox.is_empty() {
            out.append(&mut self.ecs_outbox);
        }
        out
    }

    /// Takes the counters the guest asked to raise during the last call. The
    /// host coalesces them per tick before they reach the bus.
    /// Takes the view changes the guest asked for during the last call.
    pub(crate) fn drain_view_intents(&mut self) -> Vec<crate::netcode::ViewIntent> {
        match self.wasm.as_mut() {
            Some(wasm) => std::mem::take(&mut wasm.data_mut().view_intents),
            None => Vec::new(),
        }
    }

    pub fn drain_stat_intents(&mut self) -> Vec<StatIntent> {
        match self.wasm.as_mut() {
            Some(wasm) => std::mem::take(&mut wasm.data_mut().stat_intents),
            None => Vec::new(),
        }
    }

    pub(crate) fn reload_wasm(
        &mut self,
        pre: &engine_wasm::WasmPreInstance<RoomHostData>,
        max_clients: i32,
    ) -> Result<usize, String> {
        let physics = if let Some(ref wasm) = self.wasm {
            wasm.data().physics.clone()
        } else {
            PhysicsWorld3D::default()
        };
        let mut new_wasm =
            RoomLogic::from_pre(pre, RoomHostData::new(physics)).map_err(|e| e.to_string())?;
        // Hot reload must not wipe live room state: carry the RNG stream,
        // timers, spatial index, outbox, rapier sub-worlds and their handle
        // maps over from the old instance — the new one starts from a fresh
        // RoomHostData that would otherwise forget all of it.
        if let Some(mut old) = self.wasm.take() {
            let old = old.data_mut();
            let nd = new_wasm.data_mut();
            nd.rng = std::mem::replace(&mut old.rng, engine_core::RoomRng::from_u64(0));
            nd.timers = std::mem::take(&mut old.timers);
            nd.fired_timers = std::mem::take(&mut old.fired_timers);
            nd.spatial = std::mem::replace(&mut old.spatial, engine_spatial::SpatialIndex2D::new());
            nd.outbox = std::mem::take(&mut old.outbox);
            nd.kick_requests = std::mem::take(&mut old.kick_requests);
            nd.view_intents = std::mem::take(&mut old.view_intents);
            nd.clock_ticks = old.clock_ticks;
            nd.clock_ms = old.clock_ms;
            nd.physics = std::mem::replace(&mut old.physics, PhysicsWorld3D::default());
            nd.rapier2d = old.rapier2d.take();
            nd.rapier2d_next_body_id = old.rapier2d_next_body_id;
            nd.rapier2d_bodies = std::mem::take(&mut old.rapier2d_bodies);
            nd.rapier2d_next_col_id = old.rapier2d_next_col_id;
            nd.rapier2d_colliders = std::mem::take(&mut old.rapier2d_colliders);
            nd.rapier3d = old.rapier3d.take();
            nd.rapier3d_next_body_id = old.rapier3d_next_body_id;
            nd.rapier3d_bodies = std::mem::take(&mut old.rapier3d_bodies);
            nd.rapier3d_next_col_id = old.rapier3d_next_col_id;
            nd.rapier3d_colliders = std::mem::take(&mut old.rapier3d_colliders);
        }
        GameRoom::call_on_create(&mut new_wasm, max_clients, &self.create_options);
        if let Some(map) = &self.arena_2d {
            new_wasm.data_mut().load_arena_2d(map);
        }
        new_wasm.data_mut().navmesh = self.navmesh.clone();
        for (_sid, &pid) in &self.sessions {
            let (options, auth) = match self.entries.get(&pid) {
                Some(e) => (e.options.clone(), e.auth.clone()),
                None => (Vec::new(), Vec::new()),
            };
            GameRoom::call_on_join(&mut new_wasm, pid, &options, &auth);
        }
        let players = self.sessions.len();
        self.wasm = Some(new_wasm);
        self.guest_fns_probed = false;
        self.backend = RoomBackend::Wasm;
        Ok(players)
    }
}
