mod cost;
mod http;
mod merged;
mod refusals;
mod resume;
mod sdk_json;
mod server;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::OnceReceiver;
use crate::controllers::matchmake::MAX_BODY_BYTES;
use crate::dto::matchmake::{SeatResponse, json_string_field};
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::seats::SeatRegistry;
use crate::infra::signaling::SignalHub;
use crate::services::matchmake::reserve::reserve;
use crate::services::static_routes::StaticServer;
use crate::utils::matchmake::valid_room_name;
use crate::{ApiError, ApiState};
use engine_bus::Bus;
use engine_config::{ControlConfig, DEV_SEAT_SECRET, GameConfig};

pub(super) fn test_state(rps: u32, burst: u32) -> ApiState {
    test_state_with(
        rps,
        burst,
        "[{\"urls\":[\"stun:stun.l.google.com:19302\"]}]",
    )
}

pub(super) fn test_state_with(rps: u32, burst: u32, ice: &str) -> ApiState {
    let game = GameConfig::default();
    let control = ControlConfig {
        api_port: 0,
        seat_secret: DEV_SEAT_SECRET.to_string(),
        seat_ttl_secs: 300,
        rate_limit_rps: rps,
        rate_limit_burst: burst,
        ..ControlConfig::default()
    };
    let bus = Bus::new(64, 64);
    ApiState {
        control: Arc::new(control),
        game: Arc::new(game),
        bus: bus.clone(),
        seats: Arc::new(Mutex::new(SeatRegistry::new(DEV_SEAT_SECRET))),
        limiter: Arc::new(Mutex::new(IpRateLimiter::new(rps, burst))),
        signals: SignalHub::new(ice.to_string()),
        statics: Arc::new(StaticServer::new(
            std::path::Path::new("."),
            &GameConfig::default(),
        )),
        port: 8124,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        started: Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(crate::infra::session_cache::SessionCache::new(60, 3600)),
    }
}

pub(super) fn ip(n: u8) -> IpAddr {
    IpAddr::from([198, 51, 100, n])
}

pub(super) fn minted(state: &ApiState) -> u64 {
    state.seats.lock().unwrap().counts().0
}

pub(super) fn ok(
    state: &ApiState,
    ip: IpAddr,
    method: &str,
    room: &str,
    body: &str,
) -> SeatResponse {
    match reserve(state, ip, method, room, body) {
        Ok(seat) => seat,
        Err(e) => panic!("{method}/{room} refused: {} {}", e.status, e.message),
    }
}

pub(super) fn err(state: &ApiState, ip: IpAddr, method: &str, room: &str, body: &str) -> ApiError {
    match reserve(state, ip, method, room, body) {
        Ok(seat) => panic!(
            "{method}/{room} should have been refused, minted {}",
            seat.session_id
        ),
        Err(e) => e,
    }
}

pub(super) fn reserve_ok(state: &ApiState, method: &str, room: &str, body: &str) -> SeatResponse {
    ok(state, ip(1), method, room, body)
}

pub(super) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
