use super::*;

pub fn state() -> engine_api::ApiState {
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig};

    let game = GameConfig::default();
    let control = ControlConfig::default();
    let bus = Bus::new(16, 16);
    engine_api::ApiState {
        statics: Arc::new(engine_api::services::static_routes::StaticServer::new(
            Path::new("."),
            &game,
        )),
        game: Arc::new(game),
        control: Arc::new(control),
        signals: engine_api::infra::signaling::SignalHub::new("[]".into()),
        seats: Arc::new(Mutex::new(engine_api::infra::seats::SeatRegistry::new(
            "test-secret",
        ))),
        limiter: Arc::new(Mutex::new(
            engine_api::infra::rate_limit::IpRateLimiter::new(20, 40),
        )),
        port: 8124,
        status_rx: Arc::new(engine_api::OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        bus,
        started: Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(engine_api::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(engine_api::infra::session_cache::SessionCache::new(
            60, 3600,
        )),
    }
}
#[tokio::test]
pub async fn auto_registered_health_route_dispatches() {
    let app = engine_api::auto_router(state()).await;

    let response = app
        .clone()
        .oneshot(Request::get("/auto/health").body(Body::empty()).unwrap())
        .await
        .expect("dispatch");
    assert_eq!(response.status(), StatusCode::OK);

    for path in [
        "/status",
        "/api/rooms",
        "/health",
        "/v2/user",
        "/v2/inventory",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .expect("dispatch");
        let ok = if path == "/v2/user" || path == "/v2/inventory" {
            response.status() == StatusCode::UNAUTHORIZED
        } else {
            response.status() == StatusCode::OK
        };
        assert!(ok, "{path} served as {}", response.status());
    }
}
#[tokio::test]
pub async fn two_planes_serve_their_own_state() {
    let mut first = state();
    first.port = 1111;
    let mut second = state();
    second.port = 2222;

    let a = engine_api::auto_router(first).await;
    let b = engine_api::auto_router(second).await;

    for (app, port) in [(a, 1111u64), (b, 2222u64)] {
        let body = app
            .oneshot(Request::get("/api/rooms").body(Body::empty()).unwrap())
            .await
            .expect("dispatch")
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let room: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(room[0]["controlPort"].as_u64(), Some(port));
    }
}
