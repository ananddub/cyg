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
        let func = self
            .instance
            .get_typed_func::<Args, Ret>(&mut self.store, func_name)
            .map_err(|e| WasmError::FunctionNotFound {
                name: func_name.to_string(),
                reason: e.to_string(),
            })?;

        func.call(&mut self.store, args)
            .map_err(|e| WasmError::Trap(func_name.to_string(), crate::error::trap_detail(&e)))
    }
}
