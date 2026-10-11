use wasmtime::{Instance, Memory, Store, WasmParams, WasmResults};

use crate::error::WasmError;

/// A running WebAssembly instance with its own isolated memory and state.
/// Completely decoupled and generic over any host data `T`.
pub struct WasmInstance<T> {
    store: Store<T>,
    instance: Instance,
    memory: Option<Memory>,
}

impl<T> WasmInstance<T> {
    pub fn new(mut store: Store<T>, instance: Instance) -> Self {
        let memory = instance.get_memory(&mut store, "memory");
        Self {
            store,
            instance,
            memory,
        }
    }

    /// Access host data associated with this instance.
    pub fn data(&self) -> &T {
        self.store.data()
    }

    /// Mutably access host data associated with this instance.
    pub fn data_mut(&mut self) -> &mut T {
        self.store.data_mut()
    }

    /// Mutably access the underlying Wasmtime Store.
    pub fn store_mut(&mut self) -> &mut Store<T> {
        &mut self.store
    }

    /// Access the raw Wasmtime Instance handle.
    pub fn raw_instance(&self) -> &Instance {
        &self.instance
    }

    /// Obtain a typed function handle, validating types and looking up exports once.
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

    /// Arm the epoch-interruption watchdog for this instance: the instance
    /// traps if more than `ticks` engine epochs elapse during a call.
    /// Requires `WasmConfig::enable_epoch_interruption` and a background
    /// thread (or main loop) calling `WasmRuntime::increment_epoch`.
    pub fn set_epoch_deadline(&mut self, ticks: u64) {
        self.store.set_epoch_deadline(ticks);
    }

    /// Dynamically call any exported function from the WASM module.
    pub fn call<Args, Ret>(&mut self, func_name: &str, args: Args) -> Result<Ret, WasmError>
    where
        Args: WasmParams,
        Ret: WasmResults,
    {
        let func = self
            .instance
            .get_typed_func::<Args, Ret>(&mut self.store, func_name)
            .map_err(|e| WasmError::FunctionNotFound {
                name: func_name.to_string(),
                reason: e.to_string(),
            })?;

        func.call(&mut self.store, args)
            .map_err(|e| WasmError::Trap(func_name.to_string(), e.to_string()))
    }

    /// Read raw bytes from the instance's linear memory.
    pub fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>, WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data(&self.store);
        let start = offset as usize;
        let end = start.saturating_add(len as usize);

        if end > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: start,
                len: len as usize,
                size: data.len(),
            });
        }

        Ok(data[start..end].to_vec())
    }

    /// Write raw bytes into the instance's linear memory.
    pub fn write_memory(&mut self, offset: u32, bytes: &[u8]) -> Result<(), WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data_mut(&mut self.store);
        let start = offset as usize;
        let end = start.saturating_add(bytes.len());

        if end > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: start,
                len: bytes.len(),
                size: data.len(),
            });
        }

        data[start..end].copy_from_slice(bytes);
        Ok(())
    }
}
