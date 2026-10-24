//! The plane handle and the signal its tasks shut down on.

use std::time::Duration;

use engine_api::ApiState;

/// A running control plane. Keep this alive: dropping it shuts the Tokio
/// runtime down, and with it every HTTP and signaling connection.
pub struct ControlPlane {
    pub(crate) runtime: tokio::runtime::Runtime,
    /// The plane's shared state — read by the game loop for the signaling hub.
    pub(crate) state: ApiState,
    /// The bind port the kernel actually handed us, so a caller asking for
    /// port `0` in a test can find out where it landed.
    pub port: u16,
}

impl ControlPlane {
    /// Stop the plane and shut connections down cleanly. Bounded because a
    /// hung client must not turn Ctrl-C into a hang.
    pub fn shutdown(self) {
        self.runtime.shutdown_timeout(Duration::from_secs(2));
    }

    /// The WebRTC signaling hub: browser offers queue here from `/ws/signal`,
    /// and the game loop — the data-plane owner — drains them.
    pub fn signals(&self) -> engine_api::infra::signaling::SignalHub {
        self.state.signals.clone()
    }
}

pub(crate) async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
