//! `spawn`: build the runtime, bind the listener, start the plane's tasks.

use std::path::Path;
use std::sync::Arc;

use engine_api::{ApiState, gc_seats, serve, status};
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
    let state = ApiState::new(project_dir, game, control, bus, port);

    runtime.spawn(serve(listener, state.clone(), shutdown_signal()));

    // One consumer of the bus status queue, so `/status` always has the newest
    // sample without touching the simulation thread's lock-free path.
    runtime.spawn(status::pump(state.clone()));

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
                Ok(sent) => eprintln!(
                    "[agones] 🛑 {} shutdown sent; this pod leaves the pool",
                    if sent { "first" } else { "repeat" }
                ),
                Err(e) => eprintln!("[agones] ✖ could not retire the pod: {e}"),
            }
        });
    }

    Ok(ControlPlane { runtime, port })
}
