use wasmtime::Engine;

use crate::config::WasmConfig;

mod build;
mod cache;

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
    config: WasmConfig,
}
