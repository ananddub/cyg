use std::time::Duration;

use engine_agones::{
    AgonesClient, AgonesController, Reply, SidecarConfig, SidecarError, SidecarErrorKind,
};

use crate::harness::{FakeSidecar, closed_port, framed, hang_up, ok};

#[test]
fn ready_happy_path_posts_the_agones_request_line() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let client = fake.client(Duration::from_millis(500));

    client.mark_ready().expect("/ready should be accepted");

    let request = fake.request_for("/ready").expect("the sidecar saw /ready");
    assert_eq!(request.method, "POST");
    assert_eq!(request.version, "HTTP/1.1");
    assert!(
        request.head.contains("Connection: close"),
        "{}",
        request.head
    );
    assert!(
        request.head.contains("Content-Length: 0"),
        "{}",
        request.head
    );
    assert_eq!(fake.paths(), vec!["/ready".to_string()]);
}

#[test]
fn a_rejected_ready_is_a_rejection_not_a_panic() {
    let fake = FakeSidecar::always(framed(
        400,
        "Bad Request",
        b"GameServer is in state Shutdown, cannot go Ready",
    ));
    let client = fake.client(Duration::from_millis(500));

    let error = client
        .mark_ready()
        .expect_err("a 400 must not look like success");

    match &error {
        SidecarError::Rejected { status, body, .. } => {
            assert_eq!(*status, 400);
            assert!(body.contains("cannot go Ready"), "{body}");
        }
        other => panic!("expected a rejection, got {other:?}"),
    }

    assert_eq!(error.kind(), SidecarErrorKind::Rejected);
    assert!(!error.kind().retryable());
}

#[test]
fn a_refused_sidecar_is_unreachable_not_rejected() {
    let client = AgonesClient::new(SidecarConfig {
        addr: closed_port(),
        timeout: Duration::from_millis(200),
    });
    let error = client.mark_ready().expect_err("nothing is listening");
    assert_eq!(error.kind(), SidecarErrorKind::Unreachable);
    assert!(
        error.message().contains("cannot reach sidecar"),
        "{}",
        error.message()
    );
}

#[test]
fn allocated_and_shutdown_use_their_own_paths() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let client = fake.client(Duration::from_millis(500));

    client.mark_allocated().expect("POST /allocated");
    client.shutdown().expect("POST /shutdown");

    assert_eq!(
        fake.paths(),
        vec!["/allocated".to_string(), "/shutdown".to_string()]
    );
    assert_eq!(fake.request_for("/allocated").unwrap().method, "POST");
    assert_eq!(fake.request_for("/shutdown").unwrap().method, "POST");
}

#[test]
fn a_success_body_is_returned_to_the_caller() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Ready\"}"));
    let reply: Reply = fake
        .client(Duration::from_millis(500))
        .call("GET", "/state")
        .expect("GET /state");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.reason, "OK");
    assert!(reply.is_success());
    assert_eq!(reply.body_lossy(), "{\"state\":\"Ready\"}");
}

#[test]
fn a_server_error_on_a_transition_keeps_its_status_code() {
    let fake = FakeSidecar::always(framed(500, "Internal Server Error", b"agent is unhappy"));
    let error = fake
        .client(Duration::from_millis(500))
        .send_health_ping()
        .unwrap_err();

    match error {
        SidecarError::Rejected { status, body, .. } => {
            assert_eq!(status, 500, "the code is the actionable part");
            assert_eq!(body, "agent is unhappy");
        }
        other => panic!("a 5xx answer is a rejection, got {other:?}"),
    }
    assert_eq!(fake.count("/health"), 1, "and one connection per attempt");
}

#[test]
fn a_peer_that_hangs_up_mid_reply_is_reported_not_fatal() {
    let fake = FakeSidecar::always(hang_up());
    let error = fake
        .client(Duration::from_millis(300))
        .mark_ready()
        .unwrap_err();
    assert_eq!(error.kind(), SidecarErrorKind::Malformed, "{error:?}");
}
