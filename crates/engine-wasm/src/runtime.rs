use std::path::Path;
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, Linker, Module, OptLevel, Store, Strategy,
};

use crate::config::WasmConfig;
use crate::error::WasmError;
use crate::instance::{WasmInstance, WasmPreInstance};

/// Pure, decoupled WebAssembly runtime engine.
#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
    config: WasmConfig,
}

impl WasmRuntime {
    /// Create a new runtime with default settings.
    pub fn new() -> Result<Self, WasmError> {
        Self::new_with_config(WasmConfig::default())
    }

    /// Create a new runtime with custom configuration.
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

    /// The configuration this runtime was created with.
    pub fn config(&self) -> &WasmConfig {
        &self.config
    }

    /// Direct access to the underlying Wasmtime Engine.
    pub fn raw_engine(&self) -> &Engine {
        &self.engine
    }

    /// Compile raw WebAssembly bytecode (.wasm) into a compiled Module.
    pub fn compile(&self, wasm_bytes: &[u8]) -> Result<Module, WasmError> {
        Module::new(&self.engine, wasm_bytes).map_err(|e| WasmError::Compile(e.to_string()))
    }

    /// Create a fresh Linker bound to this engine for registering host imports.
    pub fn create_linker<T>(&self) -> Linker<T> {
        Linker::new(&self.engine)
    }

    /// Instantiate a compiled module with a Linker and custom host data `T`.
    pub fn instantiate<T>(
        &self,
        linker: &Linker<T>,
        module: &Module,
        host_data: T,
    ) -> Result<WasmInstance<T>, WasmError> {
        let mut store = Store::new(&self.engine, host_data);
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        Ok(WasmInstance::new(store, instance))
    }

    /// Instantiate without any imports (pure standalone module).
    pub fn instantiate_standalone<T>(
        &self,
        module: &Module,
        host_data: T,
    ) -> Result<WasmInstance<T>, WasmError> {
        let linker: Linker<T> = Linker::new(&self.engine);
        self.instantiate(&linker, module, host_data)
    }

    /// Pre-instantiate a module with a Linker: validation, linking and type
    /// checks happen once; every subsequent [`WasmPreInstance::instantiate`]
    /// is much cheaper — the fast path for high room-spawn rates.
    pub fn prepare<T>(
        &self,
        linker: &Linker<T>,
        module: &Module,
    ) -> Result<WasmPreInstance<T>, WasmError> {
        let pre = linker
            .instantiate_pre(module)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        Ok(WasmPreInstance::new(self.engine.clone(), pre))
    }

    /// [`prepare`](Self::prepare) without any imports (standalone module).
    pub fn prepare_standalone<T>(&self, module: &Module) -> Result<WasmPreInstance<T>, WasmError> {
        let linker: Linker<T> = Linker::new(&self.engine);
        self.prepare(&linker, module)
    }

    /// Serialize a compiled module to a file so the ~ms Cranelift compilation
    /// can be skipped on future boots via [`Self::deserialize_module`].
    pub fn serialize_module(
        &self,
        module: &Module,
        path: impl AsRef<Path>,
    ) -> Result<(), WasmError> {
        let bytes = module
            .serialize()
            .map_err(|e| WasmError::Serialization(e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| WasmError::Serialization(e.to_string()))
    }

    /// Load a module previously written by [`Self::serialize_module`].
    ///
    /// # Safety
    /// The file must originate from a trusted source: deserialized modules are
    /// not re-validated and a tampered file can cause arbitrary behavior.
    pub unsafe fn deserialize_module(&self, path: impl AsRef<Path>) -> Result<Module, WasmError> {
        // SAFETY: caller guarantees the file comes from a trusted source.
        unsafe { Module::deserialize_file(&self.engine, path) }
            .map_err(|e| WasmError::Serialization(e.to_string()))
    }

    /// Increment the engine epoch if epoch interruption is enabled.
    pub fn increment_epoch(&self) {
        self.engine.increment_epoch();
    }
}
