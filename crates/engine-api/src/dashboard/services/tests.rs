
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::ApiState;
use crate::OnceReceiver;
use crate::rate_limit::IpRateLimiter;
use crate::seats::SeatRegistry;
use crate::signaling::SignalHub;
use crate::static_routes::StaticServer;

pub(super) fn state() -> ApiState {
    let game = GameConfig { name: "tank-3d".into(), ..GameConfig::default() };
    let control = ControlConfig { api_port: 0, ..ControlConfig::default() };
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
        started: Instant::now(),
    }
}

// HISTORY is a process global; serialize ring tests against other planes' pumps
pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

mod keys;
mod seams;
mod series;
