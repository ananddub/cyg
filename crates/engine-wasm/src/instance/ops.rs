use std::collections::HashMap;

use wasmtime::{Instance, Store, WasmParams, WasmResults};

use super::WasmInstance;
use crate::error::WasmError;

impl<T: 'static> WasmInstance<T> {
    pub fn new(mut store: Store<T>, instance: Instance) -> Self {
        let memory = instance.get_memory(&mut store, "memory");
        Self {
            store,
            instance,
            memory,
            epoch_ticks: None,
            func_cache: HashMap::new(),
        }
    }

    pub fn data(&self) -> &T {
        self.store.data()
    }

    pub fn data_mut(&mut self) -> &mut T {
        self.store.data_mut()
    }

    pub fn store_mut(&mut self) -> &mut Store<T> {
        &mut self.store
    }

    pub fn raw_instance(&self) -> &Instance {
        &self.instance
    }

    pub fn get_typed_func<Args, Ret>(
        &mut self,
        func_name: &str,
    ) -> Result<wasmtime::TypedFunc<Args, Ret>, WasmError>
    where
        Args: WasmParams,
        Ret: WasmResults,
    {
        self.instance
            .get_typed_func::<Args, Ret>(&mut self.store, func_name)
            .map_err(|e| WasmError::FunctionNotFound {
                name: func_name.to_string(),
                reason: e.to_string(),
            })
    }

    pub fn set_epoch_deadline(&mut self, ticks: u64) {
        self.epoch_ticks = Some(ticks);
        self.store.set_epoch_deadline(ticks);
    }

    pub fn epoch_deadline_ticks(&self) -> Option<u64> {
        self.epoch_ticks
    }

    pub fn rearm_epoch_call(&mut self) {
        if let Some(ticks) = self.epoch_ticks {
            self.store.set_epoch_deadline(ticks);
        }
    }

    pub fn call<Args, Ret>(&mut self, func_name: &str, args: Args) -> Result<Ret, WasmError>
    where
        Args: WasmParams,
        Ret: WasmResults,
    {
        self.rearm_epoch_call();
        // Cache the export lookup: only the name→Func map miss hits the
        // instance's export table; the per-call `typed::<Args, Ret>()` check
        // is a cheap signature comparison against static types.
        let func = match self.func_cache.get(func_name) {
            Some(f) => *f,
            None => {
                let f = self
                    .instance
                    .get_func(&mut self.store, func_name)
                    .ok_or_else(|| WasmError::FunctionNotFound {
                        name: func_name.to_string(),
                        reason: "export not found".into(),
                    })?;
                self.func_cache.insert(func_name.into(), f);
                f
            }
        };
        let typed =
            func.typed::<Args, Ret>(&mut self.store)
                .map_err(|e| WasmError::FunctionNotFound {
                    name: func_name.to_string(),
                    reason: e.to_string(),
                })?;

        typed
            .call(&mut self.store, args)
            .map_err(|e| WasmError::Trap(func_name.to_string(), crate::error::trap_detail(&e)))
    }
}
