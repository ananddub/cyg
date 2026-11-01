use crate::rooms::*;

impl GameRoom {

    #[inline]
    pub(crate) fn is_at_capacity(&self, max_clients: i32) -> bool {
        self.is_relay() && max_clients > 0 && self.sessions.len() as i32 >= max_clients
    }

    pub(crate) fn is_full(&mut self, max_clients: i32) -> bool {
        if let Some(full) = self.guest_is_full(self.sessions.len() as i32) {
            return full;
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

    pub(crate) fn authorize(&mut self, id: i64, options: &str) -> bool {
        self.guest_on_auth(id, options).unwrap_or(true)
    }

    fn guest_on_auth(&mut self, id: i64, options: &str) -> Option<bool> {
        let wasm = self.wasm.as_mut()?;
        if wasm
            .raw_instance()
            .get_typed_func::<(i64, i32, i32), i32>("on_auth")
            .is_err()
        {
            return None;
        }
        let bytes = options.as_bytes();
        let len = i32::try_from(bytes.len()).ok()?;
        let ptr = if len == 0 {
            0
        } else {
            let ptr = wasm.on_alloc(len).ok()?;
            wasm.write_memory(ptr as u32, bytes).ok()?;
            ptr
        };
        let instance = wasm.raw_instance();
        let func = instance
            .get_typed_func::<(i64, i32, i32), i32>("on_auth")
            .ok()?;
        func.call(instance.store_mut(), (id, ptr, len))
            .ok()
            .map(|v| v != 0)
    }

    pub(crate) fn guest_allow_reconnection(&mut self, id: i64) -> Option<i32> {
        let wasm = self.wasm.as_mut()?;
        let instance = wasm.raw_instance();
        let func = instance
            .get_typed_func::<(i64,), i32>("on_allow_reconnection")
            .ok()?;
        func.call(instance.store_mut(), (id,)).ok()
    }

    pub(crate) fn guest_patch_rate_ms(&mut self) -> i32 {
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        let instance = wasm.raw_instance();
        let Ok(func) = instance.get_typed_func::<(), i32>("on_patch_rate") else {
            return 0;
        };
        func.call(instance.store_mut(), ()).unwrap_or(0).max(0)
    }

    pub(crate) fn guest_simulation_rate_ms(&mut self) -> i32 {
        let Some(wasm) = self.wasm.as_mut() else {
            return 0;
        };
        let instance = wasm.raw_instance();
        let Ok(func) = instance.get_typed_func::<(), i32>("on_simulation_rate") else {
            return 0;
        };
        func.call(instance.store_mut(), ()).unwrap_or(0).max(0)
    }

}
