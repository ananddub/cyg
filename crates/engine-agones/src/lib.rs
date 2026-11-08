mod client;
mod error;
mod health;
mod lifecycle;
mod state;

pub use client::{AgonesClient, Reply, SidecarConfig};
pub use error::{SidecarError, SidecarErrorKind};
pub use health::{Healthbeat, PingOutcome, PingStats, UNHEALTHY_AFTER_MISSES, next_due_after};
pub use lifecycle::{
    AgonesSnapshot, Lifecycle, LifecycleEvent, SidecarErrorInfo, active, install, shutdown_sidecar,
    snapshot, stop_health_loop,
};
pub use state::{AgonesController, ServerState, StateSnapshot};

pub const DEFAULT_SIDECAR_PORT: u16 = 9358;

pub fn sidecar_port_from_env() -> u16 {
    std::env::var("AGONES_SDK_HTTP_PORT")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_SIDECAR_PORT)
}

#[cfg(test)]
mod tests;
