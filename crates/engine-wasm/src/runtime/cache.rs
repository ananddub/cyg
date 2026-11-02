use std::path::Path;
use std::time::Duration;

use wasmtime::Module;

use super::WasmRuntime;
use crate::epoch::EpochTicker;
use crate::error::WasmError;

impl WasmRuntime {
    pub fn precompile_to_file(
        &self,
        wasm_bytes: &[u8],
        out_cwasm_path: impl AsRef<Path>,
    ) -> Result<(), WasmError> {
        let module = self.compile(wasm_bytes)?;
        self.serialize_module(&module, out_cwasm_path)
    }

    pub fn load_or_compile_cached(&self, wasm_path: impl AsRef<Path>) -> Result<Module, WasmError> {
        let wasm_path = wasm_path.as_ref();

        if wasm_path.extension().and_then(|s| s.to_str()) == Some("cwasm") {
            return unsafe { self.deserialize_module(wasm_path) };
        }

        let cwasm_path = wasm_path.with_extension("cwasm");

        if let (Ok(wasm_meta), Ok(cwasm_meta)) = (wasm_path.metadata(), cwasm_path.metadata()) {
            if let (Ok(wasm_mtime), Ok(cwasm_mtime)) = (wasm_meta.modified(), cwasm_meta.modified())
            {
                if cwasm_mtime >= wasm_mtime {
                    if let Ok(module) = unsafe { self.deserialize_module(&cwasm_path) } {
                        return Ok(module);
                    }
                }
            }
        }

        let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
            WasmError::Compile(format!("Failed to read {}: {e}", wasm_path.display()))
        })?;
        let module = self.compile(&wasm_bytes)?;

        let _ = self.serialize_module(&module, &cwasm_path);

        Ok(module)
    }

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

    pub unsafe fn deserialize_module(&self, path: impl AsRef<Path>) -> Result<Module, WasmError> {
        unsafe { Module::deserialize_file(&self.engine, path) }
            .map_err(|e| WasmError::Serialization(e.to_string()))
    }

    pub fn increment_epoch(&self) {
        self.engine.increment_epoch();
    }

    pub fn spawn_epoch_ticker(&self, period: Duration) -> EpochTicker {
        EpochTicker::spawn(self.engine.clone(), period)
    }
}
