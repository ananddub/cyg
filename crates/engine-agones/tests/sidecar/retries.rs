use std::time::Duration;

use engine_agones::{AgonesSnapshot, Lifecycle, PingStats, ServerState, SidecarConfig};

use crate::harness::{FakeSidecar, closed_port, framed, ok, ready_retries_seen};

#[test]
fn the_global_snapshot_says_not_deployed_when_nothing_is_installed() {
    let snap = AgonesSnapshot::disabled();
    assert!(!snap.enabled);
    assert_eq!(snap.state, ServerState::Unknown);
    assert!(snap.addr.is_empty());
    assert_eq!(snap.pings, PingStats::default());

    let lifecycle = Lifecycle::new(
        SidecarConfig {
            addr: closed_port(),
            timeout: Duration::from_millis(20),
        },
        Duration::from_millis(2000),
    );
    let json = serde_json::to_value(lifecycle.snapshot()).expect("a JSON object");
    assert_eq!(json["enabled"], serde_json::json!(true));
    assert_eq!(json["intervalMs"], serde_json::json!(2000));
    assert_eq!(json["consecutiveFailures"], serde_json::json!(0));
}

#[test]
fn a_pod_that_was_never_ready_is_asked_again_once_the_sidecar_answers() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(200)),
        Duration::from_millis(30),
    );

    let retries = ready_retries_seen(&lifecycle, Duration::from_millis(200));

    assert_eq!(
        retries,
        vec![None],
        "exactly one retry, and it was accepted"
    );
    assert!(
        lifecycle.snapshot().ready,
        "the loop settled the lifecycle without the caller's help"
    );
    assert_eq!(lifecycle.snapshot().state, ServerState::Ready);

    let paths = fake.paths();
    assert_eq!(
        paths.first().map(String::as_str),
        Some("/health"),
        "{paths:?}"
    );
    assert_eq!(
        fake.count("/ready"),
        1,
        "and it is not asked again once accepted"
    );
    assert!(
        fake.count("/health") >= 2,
        "the beat kept its own cadence: {paths:?}"
    );
}

#[test]
fn a_rejected_ready_is_never_asked_again() {
    let fake = FakeSidecar::routing(
        vec![(
            "/ready",
            framed(400, "Bad Request", b"GameServer is Shutdown"),
        )],
        ok(b"{}"),
    );
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(200)),
        Duration::from_millis(30),
    );

    let retries = ready_retries_seen(&lifecycle, Duration::from_millis(200));

    assert_eq!(
        retries,
        vec![Some("rejected".to_string())],
        "reported, then dropped"
    );
    assert_eq!(
        fake.count("/ready"),
        1,
        "a rejection stops the retries for good"
    );
    assert!(
        fake.count("/health") >= 2,
        "the beat did not stop on a rejected /ready"
    );
    let snap = lifecycle.snapshot();
    assert!(!snap.ready, "a refused pod is not in the pool");

    assert!(lifecycle.ready_refused());
}
