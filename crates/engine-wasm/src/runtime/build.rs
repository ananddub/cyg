use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, Linker, Module, OptLevel, Store, Strategy,
};

use super::WasmRuntime;
use crate::config::WasmConfig;
use crate::error::WasmError;
use crate::instance::{WasmInstance, WasmPreInstance};

impl WasmRuntime {
    pub fn new() -> Result<Self, WasmError> {
        Self::new_with_config(WasmConfig::default())
    }

    pub fn new_with_config(cfg: WasmConfig) -> Result<Self, WasmError> {
        let mut config = Config::new();
        config.strategy(Strategy::Cranelift);

        if cfg.optimize_for_speed {
            config.cranelift_opt_level(OptLevel::Speed);
        }

        if cfg.enable_epoch_interruption {
            config.epoch_interruption(true);
        }

        if cfg.use_pooling_allocator {
            config.allocation_strategy(InstanceAllocationStrategy::pooling());
        }

        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            config: cfg,
        })
    }

    pub fn config(&self) -> &WasmConfig {
        &self.config
    }

    pub fn raw_engine(&self) -> &Engine {
        &self.engine
    }

    pub fn compile(&self, wasm_bytes: &[u8]) -> Result<Module, WasmError> {
        Module::new(&self.engine, wasm_bytes).map_err(|e| WasmError::Compile(e.to_string()))
    }

    pub fn create_linker<T>(&self) -> Linker<T> {
        Linker::new(&self.engine)
    }

    pub fn instantiate<T: 'static>(
        &self,
        linker: &Linker<T>,
        module: &Module,
        host_data: T,
    ) -> Result<WasmInstance<T>, WasmError> {
        let mut store = Store::new(&self.engine, host_data);
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        let mut spawned = WasmInstance::new(store, instance);
        if let Some(ticks) = self.config.epoch_deadline_ticks {
            spawned.set_epoch_deadline(ticks);
        }

        Ok(spawned)
    }

    pub fn instantiate_standalone<T: 'static>(
        &self,
        module: &Module,
        host_data: T,
    ) -> Result<WasmInstance<T>, WasmError> {
        let linker: Linker<T> = Linker::new(&self.engine);
        self.instantiate(&linker, module, host_data)
    }

    pub fn prepare<T: 'static>(
        &self,
        linker: &Linker<T>,
        module: &Module,
    ) -> Result<WasmPreInstance<T>, WasmError> {
        let pre = linker
            .instantiate_pre(module)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        Ok(WasmPreInstance::new(self.engine.clone(), pre)
            .with_epoch_deadline(self.config.epoch_deadline_ticks))
    }

    pub fn prepare_standalone<T: 'static>(
        &self,
        module: &Module,
    ) -> Result<WasmPreInstance<T>, WasmError> {
        let linker: Linker<T> = Linker::new(&self.engine);
        self.prepare(&linker, module)
    }
}
