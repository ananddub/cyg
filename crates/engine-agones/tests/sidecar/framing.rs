use std::time::{Duration, Instant};

use engine_agones::{AgonesController, ServerState, SidecarErrorKind};

use crate::harness::{FakeSidecar, chunked, close_delimited, garbage, truncated_body};

#[test]
fn a_truncated_reply_is_malformed_never_a_panic() {
    let fake = FakeSidecar::always(truncated_body());
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("100 promised bytes, 4 delivered");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
    assert!(error.message().contains("truncated"), "{}", error.message());
}

#[test]
fn a_reply_that_is_not_http_is_malformed() {
    let fake = FakeSidecar::always(garbage());
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .unwrap_err();
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
}

#[test]
fn a_close_delimited_reply_with_no_length_still_parses() {
    let fake = FakeSidecar::always(close_delimited(b"{\"state\":\"Ready\"}"));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("the body ends at the close");
    assert_eq!(snapshot.state, ServerState::Ready);
}

#[test]
fn a_chunked_reply_parses() {
    let fake = FakeSidecar::always(chunked(b"{\"state\":\"NotReady\",\"metadata\":{}}"));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("chunks decode");
    assert_eq!(snapshot.state, ServerState::NotReady);
    assert_eq!(snapshot.metadata, serde_json::json!({}));
}

#[test]
fn a_sidecar_that_accepts_but_never_answers_times_out() {
    let fake = FakeSidecar::silent();
    let started = Instant::now();
    let error = fake
        .client(Duration::from_millis(120))
        .send_health_ping()
        .expect_err("a silent peer must not hang us");
    let elapsed = started.elapsed();

    assert_eq!(error.kind(), SidecarErrorKind::Timeout, "{error:?}");
    assert!(elapsed < Duration::from_millis(900), "waited {elapsed:?}");
    assert!(
        fake.count("/health") >= 1,
        "the request was at least written"
    );
}
