use super::*;
use crate::db::{Datastore, KvBudget, schema};
use crate::services::status::StatusService;
use axum::http::StatusCode;

fn set_phase(state: &ApiState, phase: engine_bus::ServerPhase) {
    *state.latest_status.lock().unwrap() = Some(engine_bus::StatusSnapshot {
        server_phase: phase,
        ..Default::default()
    });
}

async fn probe(state: &ApiState) -> (StatusCode, serde_json::Value) {
    let response = StatusService::new().readyz(state).await;
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20)
        .await
        .expect("/readyz body");
    (
        parts.status,
        serde_json::from_slice(&bytes).expect("/readyz is JSON"),
    )
}

#[tokio::test]
async fn a_pod_is_ready_once_the_pump_owns_the_status_queue() {
    let state = state();
    let (status, body) = probe(&state).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "no pump means no live view of the sim: {body}"
    );
    assert_eq!(body["checks"]["pump"], "not running");
    assert_eq!(body["checks"]["database"], "not configured");
    assert_eq!(body["checks"]["schema"], "not configured");

    let pump = tokio::spawn(crate::infra::pump::pump(state.clone()));
    for _ in 0..400 {
        if state.status_rx.is_taken() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(state.status_rx.is_taken(), "the pump took the queue");

    set_phase(&state, engine_bus::ServerPhase::Ready);
    let (status, body) = probe(&state).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["checks"]["pump"], "running");
    assert_eq!(body["checks"]["lifecycle"], "ready");
    pump.abort();
}

#[tokio::test]
async fn a_migrated_datastore_reports_both_checks_live() {
    let mut state = state();
    state.status_rx.try_take().expect("the pump owns the queue");
    set_phase(&state, engine_bus::ServerPhase::Ready);
    let db = Arc::new(
        Datastore::open(
            "mem://",
            "readyz",
            "readyz",
            None,
            None,
            KvBudget::default(),
        )
        .await
        .expect("the embedded store opens"),
    );
    state.db = Some(db.clone());

    schema::ensure_schema(&db)
        .await
        .expect("the base DDL applies");
    let (status, body) = probe(&state).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["checks"]["database"], "live");
    assert_eq!(body["checks"]["schema"], "migrated");
}

#[tokio::test]
async fn a_store_without_the_schema_applied_is_not_ready() {
    let mut state = state();
    state.status_rx.try_take().expect("the pump owns the queue");
    set_phase(&state, engine_bus::ServerPhase::Ready);
    let db = Datastore::open(
        "mem://",
        "readyz-bare",
        "readyz-bare",
        None,
        None,
        KvBudget::default(),
    )
    .await
    .expect("the embedded store opens");
    state.db = Some(Arc::new(db));

    let (status, body) = probe(&state).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a store whose DDL never ran must stay out of rotation: {body}"
    );
    assert_eq!(body["checks"]["database"], "live");
}

#[tokio::test]
async fn a_draining_or_booting_server_is_not_ready_even_when_the_store_is_live() {
    let mut state = state();
    state.status_rx.try_take().expect("the pump owns the queue");

    set_phase(&state, engine_bus::ServerPhase::Booting);
    let (status, body) = probe(&state).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["checks"]["lifecycle"], "booting");

    set_phase(&state, engine_bus::ServerPhase::Draining);
    let (status, body) = probe(&state).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["checks"]["lifecycle"], "draining");
}
