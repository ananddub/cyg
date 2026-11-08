use crate::rooms::*;

impl GameRoom {
    #[inline]
    pub(crate) fn is_at_capacity(&self, max_clients: i32) -> bool {
        // The variant override applies to every backend — an operator cap on
        // `[rooms.x]` must hold for wasm rooms too, not just relays.
        let max = self.max_clients_override.unwrap_or(max_clients);
        max > 0 && self.sessions.len() as i32 >= max
    }

    pub(crate) fn is_full(&mut self, max_clients: i32) -> bool {
        if let Some(full) = self.guest_is_full(self.sessions.len() as i32) {
            if full {
                return true;
            }
            // The guest said "not full", but a variant's max_clients override
            // is the operator's cap — it must still apply. The guest can only
            // make the room MORE restrictive, never less.
            return self.is_at_capacity(max_clients);
        }
        self.is_at_capacity(max_clients)
    }

    fn guest_is_full(&mut self, count: i32) -> Option<bool> {
        let wasm = self.wasm.as_mut()?;
        let instance = wasm.raw_instance();
        let func = instance.get_typed_func::<(i32,), i32>("on_is_full").ok()?;
        func.call(instance.store_mut(), (count,))
            .ok()
            .map(|v| v != 0)
    }

    /// Validate the join. `auth_context` is the host-rendered JSON
    /// (`{"ip":…}`). On success the room keeps whatever `onAuth` returned —
    /// the auth data `onJoin(id, options, auth)` then receives (Colyseus's
    /// auth flow). A trap or a `false`/empty return rejects the join.
    pub(crate) fn authorize(&mut self, id: i64, options: &str, auth_context: &str) -> bool {
        if self.wasm.is_none() {
            return true; // relay / ecs rooms have no guest to ask
        }
        match self.guest_on_auth(id, options, auth_context) {
            Some(auth) => {
                self.entries.entry(id).or_default().auth = auth;
                true
            }
            None => false,
        }
    }

    /// ABI v2 `on_auth(id, opt_ptr, opt_len, ctx_ptr, ctx_len) -> i64`:
    /// returns the packed auth data (`(ptr << 32) | len`). Falls back to the
    /// ABI v1 `on_auth(id, ptr, len) -> i32` bool. `None` = rejected (trap,
    /// missing export, or an explicit deny).
    fn guest_on_auth(&mut self, id: i64, options: &str, auth_context: &str) -> Option<Vec<u8>> {
        let wasm = self.wasm.as_mut()?;
        let opt = GameRoom::write_guest_bytes(wasm, options.as_bytes(), "auth options");
        let ctx = GameRoom::write_guest_bytes(wasm, auth_context.as_bytes(), "auth context");
        let instance = wasm.raw_instance();
        if let Ok(func) = instance.get_typed_func::<(i64, i32, i32, i32, i32), i64>("on_auth") {
            let packed = func
                .call(instance.store_mut(), (id, opt.0, opt.1, ctx.0, ctx.1))
                .ok()?;
            let len = (packed & 0xFFFF_FFFF) as u32;
            let ptr = ((packed >> 32) & 0xFFFF_FFFF) as u32;
            if len == 0 {
                return Some(Vec::new());
            }
            return wasm.read_memory(ptr, len).ok();
        }
        let func = instance
            .get_typed_func::<(i64, i32, i32), i32>("on_auth")
            .ok()?;
        let allowed = func.call(instance.store_mut(), (id, opt.0, opt.1)).ok()?;
        if allowed != 0 { Some(Vec::new()) } else { None }
    }

    pub(crate) fn guest_allow_reconnection(&mut self, id: i64) -> Option<i32> {
        let wasm = self.wasm.as_mut()?;
        let instance = wasm.raw_instance();
        let func = instance
            .get_typed_func::<(i64,), i32>("on_allow_reconnection")
            .ok()?;
        func.call(instance.store_mut(), (id,)).ok()
    }

    /// Resolve the optional per-tick exports once. Missing exports stay
    /// `None` (guest opted out); a wasm swap resets `guest_fns_probed`.
    pub(crate) fn probe_guest_fns(&mut self) {
        if self.guest_fns_probed {
            return;
        }
        self.guest_fns_probed = true;
        let Some(wasm) = self.wasm.as_mut() else {
            return;
        };
        let instance = wasm.raw_instance();
        self.on_patch_rate_fn = instance.get_typed_func::<(), i32>("on_patch_rate").ok();
        self.on_simulation_rate_fn = instance
            .get_typed_func::<(), i32>("on_simulation_rate")
            .ok();
        self.on_patch_due_fn = instance.get_typed_func::<(i32,), ()>("on_patch_due").ok();
    }

    pub(crate) fn guest_patch_rate_ms(&mut self) -> i32 {
        self.probe_guest_fns();
        let Some(func) = &self.on_patch_rate_fn else {
            return 0;
        };
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        func.call(wasm.raw_instance().store_mut(), ())
            .unwrap_or(0)
            .max(0)
    }

    pub(crate) fn guest_simulation_rate_ms(&mut self) -> i32 {
        self.probe_guest_fns();
        let Some(func) = &self.on_simulation_rate_fn else {
            return 0;
        };
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        func.call(wasm.raw_instance().store_mut(), ())
            .unwrap_or(0)
            .max(0)
    }
}
