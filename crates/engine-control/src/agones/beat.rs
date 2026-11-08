use std::sync::Arc;
use std::time::Duration;

use engine_agones::{Lifecycle, LifecycleEvent, SidecarConfig, install};

use super::loud::Loudness;
use engine_api::ApiState;

pub fn spawn(state: &ApiState) {
    let cfg = &state.control.agones;
    if !cfg.enabled {
        return;
    }

    let env_port = engine_agones::sidecar_port_from_env();
    let port = if env_port != engine_agones::DEFAULT_SIDECAR_PORT {
        env_port
    } else {
        cfg.http_port
    };
    let interval = Duration::from_millis(cfg.health_interval_ms.max(200));

    let timeout = (interval / 2).clamp(Duration::from_millis(250), Duration::from_millis(2000));
    let sidecar = SidecarConfig {
        addr: format!("{}:{}", cfg.host, port),
        timeout,
    };

    let lifecycle = Lifecycle::new(sidecar, interval);
    if install(Arc::clone(&lifecycle)).is_err() {
        tracing::info!(
            "[agones] 🛰️  lifecycle already running at {}; reusing it",
            lifecycle.addr()
        );
        return;
    }
    tracing::info!(
        "[agones] 🛰️  sidecar at {}, health every {} ms ({} ms budget)",
        lifecycle.addr(),
        interval.as_millis(),
        timeout.as_millis()
    );

    let thread = {
        let lifecycle = Arc::clone(&lifecycle);
        std::thread::Builder::new()
            .name("ox-agones".into())
            .spawn(move || run(lifecycle))
    };
    if let Err(e) = thread {
        tracing::error!(
            "[agones] ✖ could not start the health thread: {e}; this pod will never be Ready"
        );
    }
}

fn run(lifecycle: Arc<Lifecycle>) {
    match lifecycle.mark_ready() {
        Ok(()) => match lifecycle.refresh_state() {
            Ok(state) => tracing::info!("[agones] ✅ GameServer is {}", state.display()),
            Err(e) => tracing::error!("[agones] /state after /ready failed: {e}"),
        },
        Err(e) => tracing::error!("[agones] ✖ /ready not accepted: {e}"),
    }

    let mut loud = Loudness::default();
    lifecycle.run_until_stopped(|event| match event {
        LifecycleEvent::Ping(outcome) => {
            if loud.should_log(&outcome) {
                match (&outcome.error, outcome.ok) {
                    (Some(e), false) => tracing::info!(
                        "[agones] 🩺 ping {} in a row: [{kind}] {e}",
                        outcome.consecutive_failures,
                        kind = e.kind()
                    ),
                    _ => tracing::debug!("[agones] 🩺 heartbeats recovered after a failure streak"),
                }
            }
        }
        LifecycleEvent::ReadyRetried(error) => match error {
            None => tracing::warn!("[agones] ✅ /ready accepted on retry; in the allocation pool"),
            Some(e) => tracing::error!("[agones] ✖ /ready retry refused: {e}"),
        },
    });
    let stats = lifecycle.stats();
    tracing::info!(
        "[agones] beat loop stopped; {} pings, {} failures",
        stats.pings,
        stats.failures
    );
}
