//! # Engine Agones
//!
//! A real client for the Agones Dedicated Game Server SDK sidecar: `Ready`, the
//! health ping loop, `Allocated`, `State` and `Shutdown`.
//!
//! ## The surface
//!
//! An Agones pod ships an SDK sidecar on loopback (`AGONES_SDK_HTTP_PORT`, 9358
//! by default) that speaks a handful of HTTP routes:
//!
//! ```text
//! POST /ready      → join the allocation pool
//! POST /health     → one heartbeat; three missed intervals mark the pod Unhealthy
//! POST /allocated  → record the allocation handoff
//! GET  /state      → {"state":"Ready","metadata":{…}}
//! POST /shutdown   → retire this pod
//! ```
//!
//! ## The layers
//!
//! * [`AgonesClient`] + [`SidecarConfig`] — the transport. `std::net` only, one
//!   short-lived connection per call, and a reply parser that handles
//!   `Content-Length`, chunked and close-delimited bodies without ever panicking
//!   on a truncated or non-UTF-8 response.
//! * [`AgonesController`] — the lifecycle verbs, so anything else (a test double,
//!   a gRPC client for a cluster that only speaks gRPC) can be swapped in.
//! * [`Healthbeat`] — cadence: the interval, the doubling backoff, the failure
//!   streak, and a schedule that does not drift when a ping is slow.
//! * [`Lifecycle`] — all three wired together behind `&self` methods, plus the
//!   [`AgonesSnapshot`] that a status route reports.
//!
//! ## Why `std` and no runtime
//!
//! The sidecar is a loopback HTTP peer whose latency is unbounded — a throttled
//! kubelet is a real thing. A blocking client on a dedicated OS thread cannot
//! stall an async worker, which is exactly how `src/control/agones.rs` uses it.
//! Nothing here needs a reactor, so nothing here can be the reason a tick was
//! late.
//!
//! ## The two failures are not the same failure
//!
//! [`SidecarError::Unreachable`] means there is no sidecar to talk to: wrong
//! port, container not started, not running on Kubernetes at all.
//! [`SidecarError::Rejected`] means the sidecar is fine and *this transition is
//! not valid in the GameServer's current state*. A client that logs both as
//! "agones failed" tells an operator nothing. Both are reported through
//! [`AgonesSnapshot`] with their kind, so `/status` can distinguish a
//! misconfigured pod from a pod that already moved on.
//!
//! ## Stopping
//!
//! [`Lifecycle::shutdown`] is idempotent and [`shutdown_sidecar`] is the
//! process-wide entry point for it: call it from the operator's stop path (a
//! signal handler, a graceful-drain hook) before the process exits. The health
//! loop ends on [`stop_health_loop`] or as part of the shutdown. A process that
//! exits without calling them simply stops pinging, which is what Agones treats
//! as an unhealthy pod that went away — correct, but noisier than needed.
//!
//! ```no_run
//! use std::time::Duration;
//! use engine_agones::{Lifecycle, LifecycleEvent, SidecarConfig};
//!
//! let lifecycle = Lifecycle::new(
//!     SidecarConfig::loopback(9358),
//!     Duration::from_millis(2000),
//! );
//! // Ready first: until the sidecar accepts it, no GameServer can be allocated.
//! let _ = lifecycle.mark_ready();
//! // On a thread of your own: pings forever until `stop`, retrying `/ready` if a
//! // heartbeat proves the sidecar came up after us.
//! lifecycle.run_until_stopped(|event| match event {
//!     LifecycleEvent::Ping(outcome) => eprintln!("ping ok={} {outcome:?}", outcome.ok),
//!     LifecycleEvent::ReadyRetried(error) => eprintln!("ready retried: {error:?}"),
//! });
//! // The operator's stop path:
//! let _ = lifecycle.shutdown();
//! ```

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

/// The port Agones injects as `AGONES_SDK_HTTP_PORT` when it does not say
/// otherwise.
pub const DEFAULT_SIDECAR_PORT: u16 = 9358;

/// Read `AGONES_SDK_HTTP_PORT`, falling back to [`DEFAULT_SIDECAR_PORT`]. A pod
/// that sets the port for the sidecar must not need it duplicated in config too.
pub fn sidecar_port_from_env() -> u16 {
    std::env::var("AGONES_SDK_HTTP_PORT")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_SIDECAR_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_state_parses_the_sidecar_spellings() {
        assert_eq!(ServerState::parse("Ready"), ServerState::Ready);
        assert_eq!(ServerState::parse("notReady"), ServerState::NotReady);
        assert_eq!(ServerState::parse(" NOT_READY "), ServerState::NotReady);
        assert_eq!(ServerState::parse("Allocated"), ServerState::Allocated);
        assert_eq!(ServerState::parse("Shutdown"), ServerState::Shutdown);
        assert_eq!(ServerState::parse("Scheduled"), ServerState::Scheduled);
        // A name from a future Agones release is Unknown, not a crash.
        assert_eq!(ServerState::parse("Teleported"), ServerState::Unknown);
        assert_eq!(ServerState::parse(""), ServerState::Unknown);
        assert_eq!(ServerState::NotReady.as_str(), "NotReady");
        assert_eq!(ServerState::Ready.to_string(), "Ready");
        assert!(ServerState::Ready.health_matters());
        assert!(!ServerState::Shutdown.health_matters());
    }

    #[test]
    fn state_snapshot_display_keeps_the_raw_name() {
        let snap = StateSnapshot {
            state: ServerState::Unknown,
            state_raw: "Teleported".into(),
            metadata: serde_json::json!({}),
        };
        assert_eq!(snap.display(), "Unknown(Teleported)");
        let ready = StateSnapshot {
            state: ServerState::Ready,
            state_raw: "Ready".into(),
            metadata: serde_json::json!({"name": "pod-1"}),
        };
        assert_eq!(ready.display(), "Ready");
    }

    #[test]
    fn error_kinds_are_distinct_and_named() {
        let no_sidecar = SidecarError::Unreachable {
            addr: "127.0.0.1:9358".into(),
            reason: "Connection refused (os error 111)".into(),
        };
        let rejected = SidecarError::Rejected {
            status: 400,
            reason: "Bad Request".into(),
            body: "GameServer is Shutdown".into(),
        };
        assert_eq!(no_sidecar.kind(), SidecarErrorKind::Unreachable);
        assert_eq!(rejected.kind(), SidecarErrorKind::Rejected);
        assert!(no_sidecar.kind().retryable());
        assert!(!rejected.kind().retryable());
        assert!(no_sidecar.to_string().contains("cannot reach sidecar"));
        assert!(rejected.to_string().contains("GameServer is Shutdown"));
        let as_error: &dyn std::error::Error = &rejected;
        assert!(as_error.to_string().contains("400"));
    }

    #[test]
    fn env_port_falls_back_to_the_agones_default() {
        // The variable is usually unset; either way this must not panic.
        let port = sidecar_port_from_env();
        assert!(port > 0);
        assert_eq!(DEFAULT_SIDECAR_PORT, 9358);
    }
}
