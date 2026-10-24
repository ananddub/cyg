//! `/v2` over a real socket, with no database behind it: account routes answer their
//! checks *before* touching the schema, so this harness observes the wording and order
//! a client branches on; a passed request answers `Unavailable`.

mod authenticate;
mod console;
mod games;
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

/// The handler state of an account plane with no DSN configured: `db` and `q` are
/// both `None`, which is what makes 503 the sound of a request that got through.
pub(super) fn v2_state() -> ApiState {
    let session = SessionConfig::default();
    // Tokens are verified against process-wide keys, so the plane has to be
    // configured before anything mints one — exactly as `ApiState::new` does.
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
        sessions: Arc::new(SessionCache::new(60, 3600)),
    }
}

/// `Authorization: Basic base64("<server key>:")` — the credential `Authenticate*`
/// and `SessionRefresh` demand, and no player token can stand in for it.
pub(super) fn server_key() -> String {
    let raw = format!("{}:", SessionConfig::default().server_key);
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// A player token for an account no table has ever heard of. Every route these tests
/// call answers before it looks an account up, so the id never has to exist.
pub(super) fn player_token() -> String {
    let (token, _) = jwt::mint_token_pair(Uuid::new_v4(), "player", HashMap::new());
    format!("Bearer {token}")
}
