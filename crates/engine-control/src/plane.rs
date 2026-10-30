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
    /// The hosted game's active map assets, fetched from `game_maps` while the
    /// plane came up (empty without a database or uploaded maps).
    pub(crate) boot_maps: engine_config::BootMaps,
    /// The hosted game's active room module, fetched from `room_modules` and
    /// the blob store while the plane came up (`None` without a database, an
    /// uploaded module, or readable bytes).
    pub(crate) boot_module: Option<engine_config::BootModule>,
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

    /// The hosted game's active maps, fetched once while the plane came up.
    /// Empty unless a database is configured and the game uploaded maps —
    /// local project files are the fallback either way.
    pub fn boot_maps(&self) -> engine_config::BootMaps {
        self.boot_maps.clone()
    }

    /// The hosted game's active room module, fetched once while the plane came
    /// up. `None` means the host keeps its locally compiled module.
    pub fn boot_module(&self) -> Option<engine_config::BootModule> {
        self.boot_module.clone()
    }
}

/// Wait until the process is told to stop. `ctrl_c` covers SIGINT — a terminal
/// Ctrl-C — while Kubernetes and a systemd stop send SIGTERM, which without a
/// listener would kill the game loop mid-tick with no room ever saving.
pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {
                _ = async { let _ = tokio::signal::ctrl_c().await; } => {}
                _ = terminate.recv() => {}
            }
        }
        // A blocked or unusable SIGTERM still leaves Ctrl-C to drain on.
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
