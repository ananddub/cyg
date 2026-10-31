use crate::ApiState;
use crate::OnceReceiver;
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::seats::SeatRegistry;
use crate::infra::signaling::SignalHub;
use crate::services::static_routes::StaticServer;
use axum::response::Response;
use engine_bus::{Bus, StatusSnapshot, clock::mono_ns};
use engine_config::{ControlConfig, GameConfig};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod counters;
mod pump;
mod rooms;
mod status_json;

pub(super) fn state() -> ApiState {
    let game = GameConfig {
        name: "tank-3d".into(),
        ..GameConfig::default()
    };
    let control = ControlConfig {
        api_port: 0,
        ..ControlConfig::default()
    };
    let bus = Bus::new(16, 16);
    ApiState {
        statics: Arc::new(StaticServer::new(Path::new("."), &game)),
        game: Arc::new(game),
        control: Arc::new(control),
        signals: SignalHub::new("[]".into()),
        seats: Arc::new(Mutex::new(SeatRegistry::new("test-secret"))),
        limiter: Arc::new(Mutex::new(IpRateLimiter::new(20, 40))),
        port: 8124,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        bus,
        started: Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(crate::infra::session_cache::SessionCache::new(60, 3600)),
    }
}

pub(super) async fn json_of(response: Response) -> serde_json::Value {
    let (_, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 4 << 20)
        .await
        .expect("/status body");
    serde_json::from_slice(&bytes).expect("/status is JSON")
}
