
use std::path::Path;
use std::sync::Arc;

use engine_api::{ApiState, gc_seats, pump, pump_events, serve, sweep_match_history};
use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::plane::{ControlPlane, shutdown_signal};
use crate::secrets::{describe_boot_maps, refuse_default_secrets};

pub fn spawn(
    project_dir: &Path,
    game: &GameConfig,
    control: &ControlConfig,
    bus: Arc<Bus>,
) -> std::io::Result<ControlPlane> {
    refuse_default_secrets(game, control)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(control.workers.max(1))
        .thread_name("ox-control")
        .enable_all()
        .build()?;

    let listener = runtime.block_on(tokio::net::TcpListener::bind((
        control.bind_addr.as_str(),
        control.api_port,
    )))?;
    let port = listener.local_addr()?.port();

    let state = runtime.block_on(ApiState::new(project_dir, game, control, bus, port));

    runtime.spawn(serve(listener, state.clone(), shutdown_signal()));

    let boot_maps = runtime.block_on(engine_api::services::maps::MapService::load_boot_maps(
        &state,
    ));
    if !boot_maps.is_empty() {
        tracing::info!(
            "[control] 🗺️  active maps from console: {}",
            describe_boot_maps(&boot_maps)
        );
    }

    let boot_module = runtime.block_on(engine_api::services::wasm::WasmService::load_boot_module(
        &state,
    ));
    if let Some(module) = boot_module.as_ref() {
        tracing::info!(
            "[control] 📦 active room module from console: v{} ({} bytes, sha256 {}…)",
            module.version,
            module.bytes.len(),
            &module.sha256[..module.sha256.len().min(12)]
        );
    }

    runtime.spawn(pump(state.clone()));

    runtime.spawn(gc_seats(state.clone()));

    runtime.spawn(pump_events(state.clone()));
    runtime.spawn(sweep_match_history(state.clone()));

    crate::agones::spawn(&state);

    {
        let state = state.clone();
        let agones = control.agones.enabled;
        runtime.spawn(async move {
            shutdown_signal().await;
            state.bus.request_drain();
            if agones {
                match crate::agones::shutdown() {
                    Ok(sent) => tracing::info!(
                        "[agones] 🛑 {} shutdown sent; this pod leaves the pool",
                        if sent { "first" } else { "repeat" }
                    ),
                    Err(e) => tracing::error!("[agones] ✖ could not retire the pod: {e}"),
                }
            }
        });
    }

    Ok(ControlPlane {
        runtime,
        state,
        port,
        boot_maps,
        boot_module,
    })
}
