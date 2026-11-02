use wasmtime::{Engine, Instance, InstancePre, Memory, Store};

use crate::error::WasmError;

mod memory;
mod ops;

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

    pub fn with_epoch_deadline(mut self, ticks: Option<u64>) -> Self {
        self.epoch_ticks = ticks;
        self
    }

    pub fn instantiate(&self, host_data: T) -> Result<WasmInstance<T>, WasmError> {
        let mut store = Store::new(&self.engine, host_data);

        if self.epoch_ticks.is_some() {
            store.set_epoch_deadline(u64::MAX / 2);
        }
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

    pub fn raw_pre(&self) -> &InstancePre<T> {
        &self.pre
    }
}

pub struct WasmInstance<T: 'static> {
    store: Store<T>,
    instance: Instance,
    memory: Option<Memory>,
    epoch_ticks: Option<u64>,
}
