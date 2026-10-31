mod authenticate;
mod console;
pub(crate) mod games;
mod gateway;
mod iap;
mod links;
mod server;
mod storage;
mod storage_index;
mod unlinks;
mod wasm_upload;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use base64::Engine;
use uuid::Uuid;

use crate::ApiState;
use crate::OnceReceiver;
use crate::infra::jwt;
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::seats::SeatRegistry;
use crate::infra::session_cache::SessionCache;
use crate::infra::signaling::SignalHub;
use crate::services::static_routes::StaticServer;
use engine_bus::Bus;
use engine_config::{ControlConfig, DEV_SEAT_SECRET, GameConfig, SessionConfig};

pub(super) fn v2_state() -> ApiState {
    let session = SessionConfig::default();
    jwt::configure(&session);
    let control = ControlConfig {
        session,
        ..ControlConfig::default()
    };
    let bus = Bus::new(64, 64);
    ApiState {
        control: Arc::new(control),
        game: Arc::new(GameConfig::default()),
        bus: bus.clone(),
        seats: Arc::new(Mutex::new(SeatRegistry::new(DEV_SEAT_SECRET))),
        limiter: Arc::new(Mutex::new(IpRateLimiter::new(1000, 1000))),
        signals: SignalHub::new("[]".to_string()),
        statics: Arc::new(StaticServer::new(
            std::path::Path::new("."),
            &GameConfig::default(),
        )),
        port: 0,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        started: Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(SessionCache::new(60, 3600)),
    }
}

pub(super) fn server_key() -> String {
    let raw = format!("{}:", SessionConfig::default().server_key);
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

pub(super) fn player_token() -> String {
    let (token, _) = jwt::mint_token_pair(Uuid::new_v4(), "player", HashMap::new());
    format!("Bearer {token}")
}
