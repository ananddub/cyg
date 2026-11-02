
use std::time::Duration;

use engine_api::ApiState;

pub struct ControlPlane {
    pub(crate) runtime: tokio::runtime::Runtime,

    pub(crate) state: ApiState,

    pub port: u16,

    pub(crate) boot_maps: engine_config::BootMaps,

    pub(crate) boot_module: Option<engine_config::BootModule>,
}

impl ControlPlane {

    pub fn shutdown(self) {
        self.runtime.shutdown_timeout(Duration::from_secs(2));
    }

    pub fn signals(&self) -> engine_api::infra::signaling::SignalHub {
        self.state.signals.clone()
    }

    pub fn boot_maps(&self) -> engine_config::BootMaps {
        self.boot_maps.clone()
    }

    pub fn boot_module(&self) -> Option<engine_config::BootModule> {
        self.boot_module.clone()
    }
}

pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut terminate) => {
            tokio::select! {
                _ = async { let _ = tokio::signal::ctrl_c().await; } => {
                    tracing::info!("[control] 🛑 Ctrl-C (SIGINT) received in shutdown_signal");
                }
                _ = terminate.recv() => {
                    tracing::info!("[control] 🛑 SIGTERM received in shutdown_signal");
                }
            }
        }

        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
