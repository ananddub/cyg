use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::ApiState;
use crate::OnceReceiver;
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::seats::SeatRegistry;
use crate::infra::session_cache::SessionCache;
use crate::infra::signaling::SignalHub;
use crate::services::dashboard::frames;
use crate::services::static_routes::StaticServer;

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
        seats: Arc::new(std::sync::Mutex::new(SeatRegistry::new("test-secret"))),
        limiter: Arc::new(std::sync::Mutex::new(IpRateLimiter::new(20, 40))),
        port: 8124,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(std::sync::Mutex::new(None)),
        bus,
        started: std::time::Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(SessionCache::new(60, 3600)),
    }
}

pub(super) fn overview_keys(frame: &serde_json::Value) -> BTreeSet<String> {
    frame["overview"]
        .as_object()
        .expect("overview is an object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn hello_then_snapshot_carry_the_same_overview_keys() {
    let s = state();
    let hello = serde_json::to_value(frames::hello_frame(&s)).unwrap();
    let snapshot = serde_json::to_value(frames::snapshot_frame(&s)).unwrap();
    assert_eq!(hello["type"].as_str(), Some("hello"));
    assert_eq!(snapshot["type"].as_str(), Some("snapshot"));
    assert_eq!(
        hello["protocol"].as_u64(),
        Some(u64::from(engine_dashboard::WS_PROTOCOL))
    );
    assert!(
        snapshot.get("protocol").is_none(),
        "only hello negotiates the protocol"
    );
    assert_eq!(overview_keys(&hello), overview_keys(&snapshot));
    assert!(
        snapshot["ticks"].is_object(),
        "a snapshot must be enough to redraw the graphs"
    );
}

#[test]
fn a_frame_is_bounded_in_size() {
    let json = serde_json::to_string(&frames::snapshot_frame(&state())).unwrap();
    assert!(
        json.len() < 128 * 1024,
        "snapshot frame is {} bytes",
        json.len()
    );
}

mod live_feed;
mod live_socket;
