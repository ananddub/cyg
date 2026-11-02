use std::time::Duration;

use engine_agones::{ServerState, SidecarErrorKind};

use crate::harness::{FakeSidecar, non_utf8_body, ok};

#[test]
fn state_json_parses_with_metadata() {
    let fake = FakeSidecar::always(ok(
        b"{\"state\":\"Allocated\",\"metadata\":{\"name\":\"pod-7\",\"ip\":\"10.0.0.9\"}}",
    ));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("parses");

    assert_eq!(snapshot.state, ServerState::Allocated);
    assert_eq!(snapshot.state_raw, "Allocated");
    assert_eq!(snapshot.metadata["name"], "pod-7");
    assert_eq!(snapshot.metadata["ip"], "10.0.0.9");
    assert_eq!(fake.request_for("/state").unwrap().method, "GET");
}

#[test]
fn an_unknown_state_name_is_unknown_not_an_error() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Teleported\"}"));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("parses");
    assert_eq!(snapshot.state, ServerState::Unknown);
    assert_eq!(snapshot.display(), "Unknown(Teleported)");
}

#[test]
fn state_without_a_json_body_is_malformed() {
    let fake = FakeSidecar::always(ok(b"not json at all"));
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("unparseable JSON must be an error");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
}

#[test]
fn state_json_without_a_state_field_is_malformed() {
    let fake = FakeSidecar::always(ok(b"{\"metadata\":{\"name\":\"pod-7\"}}"));
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("a payload with no phase is not a phase");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
    assert!(
        error.message().contains("no lifecycle state"),
        "{}",
        error.message()
    );
}

#[test]
fn state_json_without_metadata_still_parses() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Ready\"}"));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("parses");
    assert_eq!(snapshot.state, ServerState::Ready);
    assert_eq!(snapshot.metadata, serde_json::json!({}));
}

#[test]
fn a_non_utf8_state_body_never_panics() {
    let fake = FakeSidecar::always(non_utf8_body());
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("invalid UTF-8 is not a state");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed, "{error:?}");
}
