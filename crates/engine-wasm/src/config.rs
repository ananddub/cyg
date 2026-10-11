/// Runtime configuration options for the WASM engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WasmConfig {
    /// Enable Cranelift optimizations (default: true).
    pub optimize_for_speed: bool,
    /// Enable epoch-based interruption (default: false).
    ///
    /// Note: this only enables the mechanism on the engine. Each instance
    /// must also set a deadline via [`crate::WasmInstance::set_epoch_deadline`]
    /// (or a background thread calling [`crate::WasmRuntime::increment_epoch`]
    /// periodically) for the watchdog to actually trigger.
    pub enable_epoch_interruption: bool,
}
