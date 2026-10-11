use wasmtime::{Config, Engine, Linker, Module, OptLevel, Store, Strategy};

use crate::config::WasmConfig;
use crate::error::WasmError;
use crate::instance::WasmInstance;

/// Pure, decoupled WebAssembly runtime engine.
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

    /// Increment the engine epoch if epoch interruption is enabled.
    pub fn increment_epoch(&self) {
        self.engine.increment_epoch();
    }
}
