use wasmtime::{Engine, Instance, InstancePre, Memory, Store, WasmParams, WasmResults};

use crate::error::WasmError;

/// A pre-instantiated module: all instantiation-time validation, linking and
/// startup work is done once up-front. Each call to [`instantiate`](Self::instantiate)
/// then spawns a room with only the cheap remaining steps — ideal when
/// creating thousands of rooms per second.
pub struct WasmPreInstance<T: 'static> {
    engine: Engine,
    pre: InstancePre<T>,
    epoch_ticks: Option<u64>,
}

impl<T: 'static> WasmPreInstance<T> {
    pub fn new(engine: Engine, pre: InstancePre<T>) -> Self {
        Self {
            engine,
            pre,
            epoch_ticks: None,
        }
    }

    /// Arm every instance spawned from this template with a per-call epoch
    /// budget of `ticks` (see [`crate::WasmConfig::epoch_deadline_ticks`]).
    pub fn with_epoch_deadline(mut self, ticks: Option<u64>) -> Self {
        self.epoch_ticks = ticks;
        self
    }

    /// Spawn a new running instance (room) from the pre-built template.
    pub fn instantiate(&self, host_data: T) -> Result<WasmInstance<T>, WasmError> {
        let mut store = Store::new(&self.engine, host_data);
        let instance = self
            .pre
            .instantiate(&mut store)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        let mut spawned = WasmInstance::new(store, instance);
        if let Some(ticks) = self.epoch_ticks {
            spawned.set_epoch_deadline(ticks);
        }
        Ok(spawned)
    }

    /// The underlying raw `wasmtime::InstancePre` handle.
    pub fn raw_pre(&self) -> &InstancePre<T> {
        &self.pre
    }
}

/// A running WebAssembly instance with its own isolated memory and state.
/// Completely decoupled and generic over any host data `T`.
pub struct WasmInstance<T: 'static> {
    store: Store<T>,
    instance: Instance,
    memory: Option<Memory>,
    epoch_ticks: Option<u64>,
}

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
    ///
    /// The budget is remembered so [`rearm_epoch_call`](Self::rearm_epoch_call)
    /// can hand it to the *next* guest call: wasmtime consumes an epoch
    /// deadline rather than refilling it, so one arming at instantiation would
    /// turn into a shared, ever-shrinking allowance for the room's whole life.
    pub fn set_epoch_deadline(&mut self, ticks: u64) {
        self.epoch_ticks = Some(ticks);
        self.store.set_epoch_deadline(ticks);
    }

    /// The per-call epoch budget this instance was armed with, if any.
    pub fn epoch_deadline_ticks(&self) -> Option<u64> {
        self.epoch_ticks
    }

    /// Refill the epoch budget ahead of a guest call. A no-op for instances that
    /// were never armed, so an unwatched room pays one branch and no atomic.
    /// The [`wasm_module!`](crate::wasm_module) wrapper and
    /// [`call`](Self::call) do this for you.
    pub fn rearm_epoch_call(&mut self) {
        if let Some(ticks) = self.epoch_ticks {
            self.store.set_epoch_deadline(ticks);
        }
    }

    /// Dynamically call any exported function from the WASM module.
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

    /// [`read_memory`](Self::read_memory) writing into a caller-owned buffer,
    /// so a host reading a delta every tick reuses one allocation instead of
    /// malloc'ing per frame. The buffer is truncated to the slice length.
    pub fn read_memory_into(
        &self,
        offset: u32,
        len: u32,
        out: &mut Vec<u8>,
    ) -> Result<(), WasmError> {
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

        out.clear();
        out.extend_from_slice(&data[start..end]);
        Ok(())
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

    /// Access the full immutable raw linear memory slice of the instance.
    pub fn raw_memory_slice(&self) -> Result<&[u8], WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        Ok(memory.data(&self.store))
    }

    /// Access the full mutable raw linear memory slice of the instance.
    pub fn raw_memory_mut(&mut self) -> Result<&mut [u8], WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        Ok(memory.data_mut(&mut self.store))
    }

    /// Snapshot the entire linear memory buffer for rollback or serialization.
    pub fn snapshot_memory(&self) -> Result<Vec<u8>, WasmError> {
        self.raw_memory_slice().map(|s| s.to_vec())
    }

    /// Restore the linear memory directly from a previous snapshot.
    pub fn restore_memory(&mut self, snapshot: &[u8]) -> Result<(), WasmError> {
        let memory = self.memory.ok_or(WasmError::MissingMemory)?;
        let data = memory.data_mut(&mut self.store);
        if snapshot.len() > data.len() {
            return Err(WasmError::OutOfBounds {
                offset: 0,
                len: snapshot.len(),
                size: data.len(),
            });
        }
        data[..snapshot.len()].copy_from_slice(snapshot);
        Ok(())
    }
}
