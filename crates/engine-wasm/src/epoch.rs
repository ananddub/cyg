//! The engine-epoch pump.
//!
//! Wasmtime only interrupts a guest when *something* advances the engine epoch,
//! so an `on_tick` that never returns needs this ticker as its fuse: without a
//! pump, an armed deadline is simply never reached.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use wasmtime::Engine;

/// Advances one [`wasmtime::Engine`]'s epoch on a background thread.
///
/// The pump is what makes an armed [`crate::WasmInstance::set_epoch_deadline`]
/// reachable: instances are interrupted once `epoch_deadline_ticks` increments
/// have passed. Stopping or dropping it leaves armed instances uninterruptible,
/// which is the right behaviour while a process shuts down.
pub struct EpochTicker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EpochTicker {
    /// Spawn the pump for `engine`, advancing its epoch every `period`.
    pub fn spawn(engine: Engine, period: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("wasm-epoch".to_string())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(period);
                    engine.increment_epoch();
                }
            })
            .expect("spawn wasm epoch ticker");
        Self {
            stop,
            thread: Some(thread),
        }
    }

    /// Stop the pump and wait for its thread to exit.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop();
    }
}
