//! Agones sidecar lifecycle (Agent 1d owns the body of this file).
//!
//! An Agones pod ships an SDK sidecar on `127.0.0.1:AGONES_SDK_HTTP_PORT` (9358
//! by default) that accepts a tiny REST surface:
//!
//! ```text
//! POST /ready      → the GameServer becomes Ready and enters the pool
//! POST /allocated  → manual allocation handoff
//! POST /health     → one heartbeat; three missed intervals mark the pod Unhealthy
//! GET  /state      → current lifecycle state as JSON
//! POST /shutdown   → the sidecar tells the agent to retire this pod
//! ```
//!
//! The client that speaks it is [`engine_agones`]: a `std::net` HTTP/1.1
//! transport with honest failure kinds, a non-drifting [`Healthbeat`] and a
//! [`Lifecycle`] that keeps the reportable view. This file is only the control
//! plane's seam — it decides *where* the loop runs and *what it prints*.
//!
//! Frozen entry point: [`spawn`]. `lib.rs` calls it once while building the
//! plane, on the control plane's own runtime, and nothing else in this crate
//! needs to know whether a sidecar exists.
//!
//! The health loop is a plain OS thread with a `std::net` client rather than a
//! Tokio task: the sidecar is a loopback HTTP peer, its latency is unbounded
//! (a throttled kubelet is a real thing), and a blocking thread cannot stall an
//! Axum worker. Even the first `POST /ready` happens on that thread, so a pod
//! whose sidecar has not started yet cannot slow down the plane's startup.
//!
//! # Reading this from `/status`
//!
//! [`status_snapshot`] returns the live [`AgonesSnapshot`] (state, ping counts,
//! failure streak, backoff, last error with its kind) or the `enabled: false`
//! default when the lifecycle is off. It never blocks on the socket.
//!
//! # Stopping
//!
//! `lib.rs` awaits `ctrl_c` on the plane's own runtime and calls [`shutdown`], so
//! a SIGTERM'd pod stops heartbeating and tells the sidecar to retire it. The call
//! is idempotent and works after the sidecar is gone (the error is logged, not
//! retried), because a pod whose sidecar has vanished is a pod already going away.
//! The health thread deliberately holds no Tokio handle: it is a plain OS thread,
//! so dropping the runtime cannot leave a half-sent heartbeat behind.

use std::sync::Arc;
use std::time::Duration;

use engine_agones::{
    AgonesSnapshot, DEFAULT_SIDECAR_PORT, Lifecycle, LifecycleEvent, PingOutcome, SidecarConfig,
    UNHEALTHY_AFTER_MISSES, install,
};

use crate::ControlState;

/// Start the lifecycle loop. Returns immediately; a no-op when Agones is off.
pub fn spawn(state: &ControlState) {
    let cfg = &state.control.agones;
    if !cfg.enabled {
        return;
    }

    // The pod spec is authoritative about the sidecar port: Agones injects
    // `AGONES_SDK_HTTP_PORT` when it is not the default. Config still wins when
    // the variable is absent, and the choice is logged either way.
    let env_port = engine_agones::sidecar_port_from_env();
    let port = if env_port != DEFAULT_SIDECAR_PORT { env_port } else { cfg.http_port };
    let interval = Duration::from_millis(cfg.health_interval_ms.max(200));
    // Half the interval, so a stalled sidecar shows up as a `Timeout` and the
    // next heartbeat is still on schedule: three late pings is an Unhealthy pod.
    let timeout = (interval / 2).clamp(Duration::from_millis(250), Duration::from_millis(2000));
    let sidecar = SidecarConfig { addr: format!("{}:{}", cfg.host, port), timeout };

    let lifecycle = Lifecycle::new(sidecar, interval);
    if install(Arc::clone(&lifecycle)).is_err() {
        // A second control plane in one process (tests, a rebind) must not start a
        // second beat for the same pod.
        eprintln!("[agones] 🛰️  lifecycle already running at {}; reusing it", lifecycle.addr());
        return;
    }
    eprintln!(
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
        eprintln!("[agones] ✖ could not start the health thread: {e}; this pod will never be Ready");
    }
}

/// The lifecycle view `/status` serves: state, ping counts, failure streak,
/// backoff and the last error, or `enabled: false` when no sidecar is configured.
/// It reads a mutex the beat loop writes to — it never touches the socket, so a
/// stalled kubelet cannot slow a telemetry request down.
pub fn status_snapshot() -> AgonesSnapshot {
    engine_agones::snapshot()
}

/// Tell the sidecar to retire this pod and stop the beat loop.
///
/// Idempotent: `Ok(true)` means this call sent `POST /shutdown`, `Ok(false)` means
/// it was already sent. `Err` carries the sidecar's own words — a pod whose
/// sidecar is gone is a pod that is already going away, so the stop path should log
/// and move on.
pub fn shutdown() -> Result<bool, String> {
    engine_agones::shutdown_sidecar()
}

/// Stop heartbeating without asserting a shutdown transition.
#[allow(dead_code)]
pub fn stop_heartbeats() -> bool {
    engine_agones::stop_health_loop()
}

/// The thread body: Ready, then the beat forever.
///
/// The `/ready` retry lives in the crate ([`Lifecycle::run_until_stopped`] reports
/// it as [`LifecycleEvent::ReadyRetried`]) rather than here, so the loop cannot be
/// assembled two different ways depending on which thread owns it.
fn run(lifecycle: Arc<Lifecycle>) {
    // Ready first: until the sidecar accepts it the pod is not in the pool, and no
    // GameServer can be allocated to it. A refusal here is the normal start-of-pod
    // race — the sidecar container may still be starting — so it is logged and the
    // beat loop takes over, retrying once a heartbeat proves there is a sidecar.
    match lifecycle.mark_ready() {
        Ok(()) => match lifecycle.refresh_state() {
            Ok(state) => eprintln!("[agones] ✅ GameServer is {}", state.display()),
            Err(e) => eprintln!("[agones] /state after /ready failed: {e}"),
        },
        Err(e) => eprintln!("[agones] ✖ /ready not accepted: {e}"),
    }

    let mut loud = Loudness::default();
    lifecycle.run_until_stopped(|event| match event {
        LifecycleEvent::Ping(outcome) => {
            if loud.should_log(outcome) {
                match (&outcome.error, outcome.ok) {
                    (Some(e), false) => eprintln!(
                        "[agones] 🩺 ping {} in a row: [{kind}] {e}",
                        outcome.consecutive_failures,
                        kind = e.kind()
                    ),
                    _ => eprintln!("[agones] 🩺 heartbeats recovered after a failure streak"),
                }
            }
        }
        LifecycleEvent::ReadyRetried(error) => match error {
            None => eprintln!("[agones] ✅ /ready accepted on retry; in the allocation pool"),
            Some(e) => eprintln!("[agones] ✖ /ready retry refused: {e}"),
        },
    });
    let stats = lifecycle.stats();
    eprintln!("[agones] beat loop stopped; {} pings, {} failures", stats.pings, stats.failures);
}

/// Which heartbeat outcomes earn a log line.
///
/// A pod that cannot reach its sidecar logs every interval for minutes on end, and
/// a log that never changes is a log nobody reads. So: the first
/// [`UNHEALTHY_AFTER_MISSES`] misses (that is the window Agones itself uses), then
/// only the crossing of it and every doubling of the streak — plus exactly one line
/// when the streak breaks.
#[derive(Debug, Default)]
struct Loudness {
    /// Misses already logged for the current streak; `0` while healthy.
    logged: u32,
}

impl Loudness {
    fn should_log(&mut self, outcome: &PingOutcome) -> bool {
        if outcome.ok {
            // Report the recovery only if we ever reported the failure.
            let was_loud = self.logged > 0;
            self.logged = 0;
            return was_loud;
        }
        let misses = outcome.consecutive_failures;
        let noteworthy = outcome.just_missed_the_window
            || misses <= UNHEALTHY_AFTER_MISSES
            || misses.is_power_of_two();
        if noteworthy {
            self.logged = misses;
        }
        noteworthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn miss(n: u32) -> PingOutcome {
        PingOutcome {
            ok: false,
            latency: Duration::from_millis(1),
            consecutive_failures: n,
            error: Some(engine_agones::SidecarError::Unreachable {
                addr: "127.0.0.1:9358".into(),
                reason: "Connection refused (os error 111)".into(),
            }),
            just_missed_the_window: n == UNHEALTHY_AFTER_MISSES,
        }
    }

    fn beat() -> PingOutcome {
        PingOutcome {
            ok: true,
            latency: Duration::from_millis(2),
            consecutive_failures: 0,
            error: None,
            just_missed_the_window: false,
        }
    }

    #[test]
    fn loudness_reports_the_window_then_goes_quiet() {
        let mut loud = Loudness::default();
        assert!(loud.should_log(&miss(1)));
        assert!(loud.should_log(&miss(2)));
        assert!(loud.should_log(&miss(3)), "the third miss is Agones' unhealthy window");
        assert!(loud.should_log(&miss(4)));
        assert!(!loud.should_log(&miss(5)), "a streak this long must not spam every interval");
        assert!(loud.should_log(&miss(8)), "but every doubling is still news");
        assert!(loud.should_log(&beat()), "and the recovery is exactly one line");
        assert!(!loud.should_log(&beat()), "then nothing while healthy");
    }

    #[test]
    fn a_clean_run_logs_nothing() {
        let mut loud = Loudness::default();
        for _ in 0..5 {
            assert!(!loud.should_log(&beat()), "a healthy pod has no reason to print");
        }
    }

    #[test]
    fn status_snapshot_is_serveable_without_a_sidecar() {
        // Agones is off by default, so `/status` must be able to call this in any
        // process, at any time, and get JSON rather than a panic or a block.
        let snapshot = status_snapshot();
        let json = serde_json::to_value(&snapshot).expect("serialises");
        assert!(json.get("enabled").is_some(), "{json}");
        assert!(json.get("consecutiveFailures").is_some(), "{json}");
        assert!(json.get("addr").is_some(), "{json}");
        if snapshot.enabled {
            // Another test in this binary installed a lifecycle; still consistent.
            assert_eq!(json["addr"], serde_json::json!(snapshot.addr));
        } else {
            assert_eq!(json["enabled"], serde_json::json!(false));
        }
    }

    #[test]
    fn shutdown_without_a_lifecycle_is_a_told_reason_not_a_panic() {
        // Either nothing is installed (the usual test case) or a lifecycle from
        // another test is: both must return without touching a socket we do not own.
        let result = shutdown();
        assert!(result.is_ok() || result.unwrap_err().contains("no Agones lifecycle"));
    }
}
