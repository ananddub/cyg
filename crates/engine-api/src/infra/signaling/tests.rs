use super::*;
use crate::ApiState;
use crate::OnceReceiver;
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::seats::{Seat, SeatRegistry};
use crate::services::static_routes::StaticServer;
use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

mod frames;
mod queues;
mod raw_recv;
mod raw_ws;
mod reconnect;
mod refusals;
mod relay;

pub(super) const AWAIT_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn test_state() -> (ApiState, Receiver<SignalIn>) {
    let game = GameConfig {
        voice_ice_servers: r#"[{"urls":["stun:stun.l.google.com:19302"]}]"#.into(),
        ..Default::default()
    };
    let control = ControlConfig {
        seat_secret: "test-seat-secret".into(),
        ..Default::default()
    };
    let bus = Bus::new(16, 16);
    let state = ApiState {
        control: Arc::new(control),
        game: Arc::new(game.clone()),
        bus: bus.clone(),
        seats: Arc::new(Mutex::new(SeatRegistry::new("test-seat-secret"))),
        limiter: Arc::new(Mutex::new(IpRateLimiter::new(1000, 1000))),
        signals: SignalHub::new(game.voice_ice_servers.clone()),
        statics: Arc::new(StaticServer::new(Path::new("."), &game)),
        port: 0,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        started: Instant::now(),
        db: None,
        q: None,
        blobs: None,
        realtime: std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(None)),
        sessions: Arc::new(crate::infra::session_cache::SessionCache::new(60, 3600)),
    };
    let inbound = state
        .signals
        .take_signals()
        .expect("a fresh hub hands over its inbound queue");
    (state, inbound)
}

pub(super) fn mint_seat(state: &ApiState) -> Seat {
    state
        .seats
        .lock()
        .unwrap()
        .mint("tank-3d", 300)
        .expect("an empty registry mints a valid room")
}

pub(super) async fn spawn_plane(state: &ApiState) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = crate::auto_router(state.clone()).await;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

pub(super) async fn next_signal(rx: &mut Receiver<SignalIn>) -> SignalIn {
    tokio::time::timeout(AWAIT_TIMEOUT, rx.recv())
        .await
        .expect("the data-plane queue never drained")
        .expect("inbound queue closed")
}

pub(super) async fn expect_quiet(rx: &mut Receiver<SignalIn>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(250), rx.recv())
            .await
            .is_err(),
        "signaling sent the data plane more than it was owed"
    );
}

pub(super) async fn wait_peers(hub: &SignalHub, want: usize) {
    for _ in 0..100 {
        if hub.peers() == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("peers() never reached {want} (still {})", hub.peers());
}

pub(super) fn signal_path(seat: &Seat) -> String {
    format!("/ws/signal?seat={}", seat.token)
}
