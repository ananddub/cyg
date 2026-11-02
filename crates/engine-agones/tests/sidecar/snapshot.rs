use std::time::Duration;

use engine_agones::{Lifecycle, ServerState, SidecarConfig, SidecarErrorKind};

use crate::harness::{FakeSidecar, closed_port, framed, ok};

#[test]
fn lifecycle_keeps_a_rejection_in_the_snapshot() {
    let fake = FakeSidecar::always(framed(400, "Bad Request", b"GameServer is Shutdown"));
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(300)),
        Duration::from_millis(40),
    );

    let error = lifecycle.mark_ready().expect_err("rejected");
    assert_eq!(error.kind(), SidecarErrorKind::Rejected);

    let snap = lifecycle.snapshot();
    assert!(!snap.ready, "a rejected /ready never put us in the pool");
    assert_eq!(snap.state, ServerState::Unknown);
    let info = snap.last_error.expect("the rejection is reportable");
    assert_eq!(info.kind, "rejected");
    assert!(
        info.message.contains("GameServer is Shutdown"),
        "{}",
        info.message
    );
}

#[test]
fn lifecycle_state_refresh_overwrites_an_inferred_state() {
    let fake = FakeSidecar::scripted(
        vec![
            ok(b"{}"),
            ok(b"{\"state\":\"Allocated\",\"metadata\":{\"room\":\"dm\"}}"),
        ],
        ok(b"{}"),
    );
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(300)),
        Duration::from_millis(40),
    );
    lifecycle.mark_ready().expect("ready");
    let snapshot = lifecycle.refresh_state().expect("state");

    assert_eq!(snapshot.state, ServerState::Allocated);
    assert_eq!(snapshot.metadata["room"], "dm");
    assert_eq!(
        fake.paths(),
        vec!["/ready".to_string(), "/state".to_string()]
    );

    let snap = lifecycle.snapshot();
    assert_eq!(
        snap.state,
        ServerState::Allocated,
        "the view follows the sidecar, not our guess"
    );
    assert_eq!(snap.state_raw, "Allocated");
    assert_eq!(snap.last_transition, "state");
    assert!(
        snap.last_error.is_none(),
        "a good read clears the old failure"
    );
}

#[test]
fn a_failing_beat_loop_is_visible_in_the_status_snapshot() {
    let lifecycle = Lifecycle::new(
        SidecarConfig {
            addr: closed_port(),
            timeout: Duration::from_millis(30),
        },
        Duration::from_millis(40),
    );
    for _ in 0..3 {
        let outcome = lifecycle.ping_once();
        assert!(!outcome.ok);
    }
    let snap = lifecycle.snapshot();
    assert_eq!(snap.pings.consecutive_failures, 3);
    assert_eq!(snap.pings.unreachable_failures, 3);
    assert!(
        snap.pings.unhealthy_declared,
        "three misses is the Agones window"
    );
    assert_eq!(snap.backoff_ms, 160, "the doubling is reportable");
    assert_eq!(snap.interval_ms, 40);
    assert!(snap.enabled);
    let json = serde_json::to_string(&snap).expect("the payload is serialisable");
    assert!(json.contains("\"unhealthyDeclared\":true"), "{json}");
    assert!(json.contains("\"consecutiveFailures\":3"), "{json}");
}
