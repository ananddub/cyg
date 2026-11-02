use super::{DEFAULT_SIDECAR_PORT, sidecar_port_from_env};
use crate::error::{SidecarError, SidecarErrorKind};
use crate::state::{ServerState, StateSnapshot};

#[test]
fn server_state_parses_the_sidecar_spellings() {
    assert_eq!(ServerState::parse("Ready"), ServerState::Ready);
    assert_eq!(ServerState::parse("notReady"), ServerState::NotReady);
    assert_eq!(ServerState::parse(" NOT_READY "), ServerState::NotReady);
    assert_eq!(ServerState::parse("Allocated"), ServerState::Allocated);
    assert_eq!(ServerState::parse("Shutdown"), ServerState::Shutdown);
    assert_eq!(ServerState::parse("Scheduled"), ServerState::Scheduled);

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
    let port = sidecar_port_from_env();
    assert!(port > 0);
    assert_eq!(DEFAULT_SIDECAR_PORT, 9358);
}
