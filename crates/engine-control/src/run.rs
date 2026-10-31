//! `spawn`: build the runtime, bind the listener, start the plane's tasks.

use std::path::Path;
use std::sync::Arc;

use engine_api::{ApiState, gc_seats, pump, serve};
use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::plane::{ControlPlane, shutdown_signal};

/// Start the control plane on its own multi-thread Tokio runtime.
///
/// Returns once the listener is bound, so the caller can print a real port.
/// Nothing here blocks the simulation thread: the plane only ever `try_send`s
/// onto the bus.
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

    // Bind before building the state: the seat reservation and `/status` both
    // carry the real port, and `api_port: 0` means "ask the kernel".
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind((control.bind_addr.as_str(), control.api_port)))?;
    let port = listener.local_addr()?.port();
    // Opening the datastore uses the surrounding runtime (remote endpoints
    // connect over the network), so the state is built inside its context.
    let state = runtime.block_on(ApiState::new(project_dir, game, control, bus, port));

    runtime.spawn(serve(listener, state.clone(), shutdown_signal()));

    // The game's console-uploaded maps, fetched once so the host loop starts
    // with them instead of an SSH session copying files around. A plane with
    // no database (or a game with no maps) gets an empty set and falls back to
    // the project folder's local files, exactly as it always has.
    let boot_maps = runtime.block_on(engine_api::services::maps::MapService::load_boot_maps(&state));
    if !boot_maps.is_empty() {
        tracing::info!(
            "[control] 🗺️  active maps from console: {}",
            describe_boot_maps(&boot_maps)
        );
    }

    // One consumer of the bus status queue, so `/status` always has the newest
    // sample without touching the simulation thread's lock-free path.
    runtime.spawn(pump(state.clone()));

    // Expiring seats must not accumulate if nothing ever calls `verify`.
    runtime.spawn(gc_seats(state.clone()));

    // Agones sidecar lifecycle: Ready, then the Health ping loop. No-op unless
    // `[control.agones] enabled = true`.
    crate::agones::spawn(&state);

    // Ctrl-C is a pod's last words: stop the beat and tell the sidecar to
    // retire this GameServer, or the agent holds a dead pod in the Ready pool
    // while clients keep being matched into a server that no longer answers.
    if control.agones.enabled {
        runtime.spawn(async {
            shutdown_signal().await;
            match crate::agones::shutdown() {
                Ok(sent) => tracing::info!(
                    "[agones] 🛑 {} shutdown sent; this pod leaves the pool",
                    if sent { "first" } else { "repeat" }
                ),
                Err(e) => tracing::error!("[agones] ✖ could not retire the pod: {e}"),
            }
        });
    }

    Ok(ControlPlane { runtime, state, port, boot_maps })
}

/// Refuse to boot the control plane on well-known default secrets. A default-
/// configured deployment is trivially full-console-compromisable (anyone can
/// forge an admin token offline with the documented default key). `OX_ALLOW_DEFAULT_SECRETS=1`
/// is the explicit dev-only escape hatch.
fn refuse_default_secrets(game: &GameConfig, control: &ControlConfig) -> std::io::Result<()> {
    if std::env::var("OX_ALLOW_DEFAULT_SECRETS").as_deref() == Ok("1") {
        tracing::warn!("[control] ⚠️  OX_ALLOW_DEFAULT_SECRETS=1 — booting with default secrets (DEV ONLY)");
        return Ok(());
    }
    let mut defaults: Vec<&str> = Vec::new();
    let s = &control.session;
    for (name, value) in [
        ("session.signing_key", s.signing_key.as_str()),
        ("session.refresh_signing_key", s.refresh_signing_key.as_str()),
        ("session.server_key", s.server_key.as_str()),
        ("control.seat_secret", control.seat_secret.as_str()),
    ] {
        if value.starts_with("default") || value == "ox-dev-seat-secret" {
            defaults.push(name);
        }
    }
    let c = &control.console;
    if c.password == "password" || c.signing_key.starts_with("default") {
        defaults.push("control.console.*");
    }
    if !defaults.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to start: default secrets configured ({}). Set real secrets in config.toml, or export OX_ALLOW_DEFAULT_SECRETS=1 for local dev.",
                defaults.join(", ")
            ),
        ));
    }
    Ok(())
}

/// One log line's worth of `BootMaps` — which formats came from the console.
fn describe_boot_maps(maps: &engine_config::BootMaps) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if maps.oxcl.is_some() {
        parts.push("oxcl");
    }
    if maps.map2d.is_some() {
        parts.push("map2d");
    }
    if maps.map3d.is_some() {
        parts.push("map3d");
    }
    if maps.navmesh.is_some() {
        parts.push("navmesh");
    }
    parts.join(", ")
}
