use crate::netcode::deliver_input_now;
use crate::rooms::*;
use engine_physics::PhysicsWorld3D;
use std::time::Duration;

impl GameRoom {
    /// Stage a frame for tick-time batch delivery (input_coalesce). Newest
    /// frame per player wins; the seq gate in `apply_input` already ensures
    /// only newer frames reach here.
    pub(crate) fn stage_input(&mut self, id: i64, payload: [u8; 20]) {
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

    pub(crate) fn tick(
        &mut self,
        dt: f64,
        rollback_enabled: bool,
    ) -> Result<i64, engine_wasm::WasmError> {
        self.current_tick += 1;
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
