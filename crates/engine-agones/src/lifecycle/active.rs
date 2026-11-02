use std::sync::Arc;
use std::sync::OnceLock;

use super::{AgonesSnapshot, Lifecycle};

static ACTIVE: OnceLock<Arc<Lifecycle>> = OnceLock::new();

pub fn install(lifecycle: Arc<Lifecycle>) -> Result<(), Arc<Lifecycle>> {
    ACTIVE.set(lifecycle)
}

pub fn active() -> Option<Arc<Lifecycle>> {
    ACTIVE.get().cloned()
}

pub fn snapshot() -> AgonesSnapshot {
    match ACTIVE.get() {
        Some(lifecycle) => lifecycle.snapshot(),
        None => AgonesSnapshot::disabled(),
    }
}

pub fn shutdown_sidecar() -> Result<bool, String> {
    let Some(lifecycle) = active() else {
        return Err("no Agones lifecycle is running in this process".to_string());
    };

    lifecycle.stop();
    lifecycle.shutdown().map_err(|e| e.message())
}

pub fn stop_health_loop() -> bool {
    match active() {
        Some(lifecycle) => {
            lifecycle.stop();
            true
        }
        None => false,
    }
}
