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
    ///
    /// Turning this on *without* arming is fatal rather than inert: a fresh
    /// store's deadline starts at epoch 0, which has always elapsed, so every
    /// guest call would trap at its first loop check. Set
    /// [`epoch_deadline_ticks`](Self::epoch_deadline_ticks) together with this
    /// flag and let [`crate::WasmRuntime::spawn_epoch_ticker`] pump the epoch.
    pub enable_epoch_interruption: bool,
    /// Per-call epoch budget armed on every instance this runtime spawns
    /// (default: `None` = never arm). Because an epoch deadline is consumed by
    /// the call it was set for, the wrapper re-arms it before each guest call,
    /// so the budget is `Some(ticks)` slices *per call*, not per room.
    pub epoch_deadline_ticks: Option<u64>,
    /// Use Wasmtime's pooling allocator (default: false).
    ///
    /// Pre-allocates linear memory and table slots up-front so instantiation
    /// reuses pooled resources instead of mmap-ing fresh ones per room.
    /// Big win when spawning thousands of short-lived rooms per second.
    /// Trades a larger fixed memory footprint for lower per-room latency.
    pub use_pooling_allocator: bool,
}
