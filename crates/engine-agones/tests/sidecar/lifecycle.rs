use std::time::Duration;

use engine_agones::{AgonesSnapshot, Lifecycle, ServerState, SidecarConfig, SidecarErrorKind};

use crate::harness::{FakeSidecar, closed_port, ok};

#[test]
fn lifecycle_records_ready_then_pings_and_shuts_down_once() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(300)),
        Duration::from_millis(40),
    );

    lifecycle.mark_ready().expect("ready");
    assert_eq!(fake.request_for("/ready").unwrap().method, "POST");

    let outcome = lifecycle.ping_once();
    assert!(outcome.ok, "{outcome:?}");

    let snap = lifecycle.snapshot();
    assert!(snap.enabled);
    assert_eq!(snap.state, ServerState::Ready);
    assert!(snap.ready);
    assert_eq!(snap.last_transition, "ready");
    assert_eq!(snap.pings.pings, 1);
    assert_eq!(snap.pings.successes, 1);
    assert!(snap.last_error.is_none());

    assert!(lifecycle.shutdown().expect("first shutdown sends it"));
    assert!(
        !lifecycle.shutdown().expect("second shutdown is a no-op"),
        "idempotent"
    );
    assert_eq!(
        fake.count("/shutdown"),
        1,
        "the sidecar is told exactly once"
    );
    assert!(lifecycle.is_stopped(), "and the beat is done");

    let snap = lifecycle.snapshot();
    assert_eq!(snap.state, ServerState::Shutdown);
    assert!(snap.shutdown_sent);
    assert!(snap.stopped);
    assert_eq!(snap.last_transition, "shutdown");
}

#[test]
fn lifecycle_shutdown_is_retried_after_an_unreachable_sidecar() {
    let lifecycle = Lifecycle::new(
        SidecarConfig {
            addr: closed_port(),
            timeout: Duration::from_millis(50),
        },
        Duration::from_millis(40),
    );
    let error = lifecycle.shutdown().expect_err("nothing is listening");
    assert_eq!(error.kind(), SidecarErrorKind::Unreachable);

    let snap: AgonesSnapshot = lifecycle.snapshot();
    assert!(
        !snap.shutdown_sent,
        "a failed shutdown must not be recorded as sent"
    );
    assert_eq!(
        snap.last_error.map(|e| e.kind).as_deref(),
        Some("unreachable")
    );
    assert_eq!(snap.last_transition, "shutdown");
}
