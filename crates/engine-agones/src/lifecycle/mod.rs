use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::client::AgonesClient;
use crate::health::Healthbeat;
use crate::health::PingOutcome;

mod active;
mod loop_runner;
mod snapshot;
mod state_methods;

#[cfg(test)]
mod tests;

pub use active::{active, install, shutdown_sidecar, snapshot, stop_health_loop};
pub use loop_runner::LifecycleEvent;
pub use snapshot::{AgonesSnapshot, SidecarErrorInfo};

pub(crate) const SLEEP_SLICE: Duration = Duration::from_millis(20);

pub(crate) enum Step {
    Ping(PingOutcome),
    Wait(Duration),
}

pub(crate) fn poisoned() -> crate::error::SidecarError {
    crate::error::SidecarError::Malformed {
        reason: "the lifecycle's own state lock was poisoned".into(),
    }
}

#[derive(Debug)]
pub struct Lifecycle {
    pub(crate) client: AgonesClient,
    pub(crate) beat: Mutex<Healthbeat>,
    pub(crate) view: Mutex<AgonesSnapshot>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) shutdown_sent: AtomicBool,
    pub(crate) ready_refused: AtomicBool,
    pub(crate) started: Instant,
    pub(crate) interval: Duration,
}
