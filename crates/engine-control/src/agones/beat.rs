use std::sync::Arc;
use std::time::Duration;

use engine_agones::{Lifecycle, LifecycleEvent, SidecarConfig, install};

use super::loud::Loudness;
use engine_api::ApiState;

/// Start the lifecycle loop. Returns immediately; a no-op when Agones is off.
pub fn spawn(state: &ApiState) {
    let cfg = &state.control.agones;
    if !cfg.enabled {
        return;
    }

    // The pod spec is authoritative about the sidecar port: Agones injects
    // `AGONES_SDK_HTTP_PORT` when it is not the default.
    let env_port = engine_agones::sidecar_port_from_env();
    let port = if env_port != engine_agones::DEFAULT_SIDECAR_PORT { env_port } else { cfg.http_port };
    let interval = Duration::from_millis(cfg.health_interval_ms.max(200));
    // Half the interval, so a stalled sidecar shows up as a `Timeout` and the
    // next heartbeat is still on schedule: three late pings is an Unhealthy pod.
    let timeout = (interval / 2).clamp(Duration::from_millis(250), Duration::from_millis(2000));
    let sidecar = SidecarConfig { addr: format!("{}:{}", cfg.host, port), timeout };

    let lifecycle = Lifecycle::new(sidecar, interval);
    if install(Arc::clone(&lifecycle)).is_err() {
        // A second control plane in one process (tests, a rebind) must not start a
        // second beat for the same pod.
        tracing::info!("[agones] 🛰️  lifecycle already running at {}; reusing it", lifecycle.addr());
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
        std::thread::Builder::new().name("ox-agones".into()).spawn(move || run(lifecycle))
    };
    if let Err(e) = thread {
        tracing::error!("[agones] ✖ could not start the health thread: {e}; this pod will never be Ready");
    }
}

/// The thread body: Ready, then the beat forever. The `/ready` retry lives in
/// the crate (`Lifecycle::run_until_stopped` reports it as `ReadyRetried`)
/// rather than here, so the loop cannot be assembled two ways.
fn run(lifecycle: Arc<Lifecycle>) {
    // Ready first: until the sidecar accepts it the pod is not in the pool. A
    // refusal here is the normal start-of-pod race, so it is logged and the
    // beat loop takes over, retrying once a heartbeat proves a sidecar exists.
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
    tracing::info!("[agones] beat loop stopped; {} pings, {} failures", stats.pings, stats.failures);
}
