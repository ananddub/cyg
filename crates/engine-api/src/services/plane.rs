//! The long-lived context a service is built from.
//!
//! `ApiState` is the request-side bag: it is cloned into every request's
//! extensions, it carries the boot `OnceReceiver`, and it exists so the HTTP
//! layer can hand one thing to every extractor. A service has no business
//! depending on that shape — it is replaced whenever the transport changes, and
//! taking it as a parameter hides which of its twenty fields the service reads.
//!
//! `Plane` is what a service depends on instead: the handles that live as long as
//! the process, named, and injected through `#[singleton] fn new`. A service that
//! only needs the datastore takes `Arc<Store>`; one that also reads the game
//! config, the static files or the seat registry takes `Arc<Plane>`. Either way
//! the dependency is in the constructor, not in a parameter threaded through
//! every call.

use std::sync::Arc;

use engine_bus::Bus;
use engine_config::{ControlConfig, GameConfig};

use crate::db::{BlobStore, Datastore};
use crate::error::V2Error;
use crate::infra::rate_limit::IpRateLimiter;
use crate::infra::realtime::RealtimeHub;
use crate::infra::seats::SeatRegistry;
use crate::infra::session_cache::SessionCache;
use crate::infra::signaling::SignalHub;
use crate::repositories::db::Queries;
use crate::services::static_routes::StaticServer;
use crate::services::store::Store;

/// Everything a service may read, injected once at construction.
#[derive(Clone)]
pub struct Plane {
    /// The data layer: datastore, blob store, customer hooks.
    pub store: Store,
    pub game: Arc<GameConfig>,
    pub control: Arc<ControlConfig>,
    pub bus: Arc<Bus>,
    pub seats: Arc<std::sync::Mutex<SeatRegistry>>,
    pub limiter: Arc<IpRateLimiter>,
    pub signals: SignalHub,
    pub statics: Arc<StaticServer>,
    pub realtime: Arc<RealtimeHub>,
    pub sessions: Arc<SessionCache>,
    pub chain: Arc<std::sync::Mutex<crate::services::matchmake::chain::ChainTracker>>,
    pub db: Option<Arc<Datastore>>,
    /// The port this process binds, which is what a seat advertises.
    pub port: u16,
    /// The boot status channel and its latest snapshot, for `/status` and
    /// `/readyz`. They are plane-wide, not request-scoped: one reader, many
    /// readers' worth of answers.
    pub status_rx: Arc<crate::app::state::OnceReceiver>,
    pub latest_status: Arc<std::sync::Mutex<Option<engine_bus::StatusSnapshot>>>,
    /// When the process started, which is what uptime is measured from.
    pub started: std::time::Instant,
    /// Whether this is an explicitly-started local session (`cyg run -d`).
    pub dev_mode: bool,
}

impl Plane {
    /// The datastore, or `no_db`.
    pub async fn q(&self) -> Result<Arc<Queries>, V2Error> {
        self.store.q().await
    }

    pub async fn blobs(&self) -> Result<Arc<BlobStore>, V2Error> {
        self.store.blobs().await
    }

    pub fn hooks(&self) -> Option<Arc<engine_hooks::HookRunner>> {
        self.store.hooks()
    }

    /// The port the control plane advertises, which is the one it binds.
    pub fn control_port(&self) -> u16 {
        self.port
    }

    pub fn verify_seat(&self, token: &str) -> Result<crate::infra::seats::Seat, String> {
        self.seats
            .lock()
            .map_err(|_| "seat registry is unavailable".to_string())?
            .verify(token)
            .ok_or_else(|| "seat token is invalid or expired".to_string())
    }

    /// A plane for unit tests that exercise logic which reads no store and binds
    /// no port. Every handle is real, so a test that unexpectedly reaches for one
    /// gets the same behaviour it would in production rather than a panic.
    #[doc(hidden)]
    pub fn for_tests() -> Arc<Plane> {
        let bus = Bus::new(64, 64);
        Arc::new(Plane {
            store: Store::default(),
            game: Arc::new(GameConfig::default()),
            control: Arc::new(ControlConfig::default()),
            seats: Arc::new(std::sync::Mutex::new(SeatRegistry::new("test-seat-secret"))),
            limiter: Arc::new(IpRateLimiter::new(10_000, 10_000)),
            signals: SignalHub::new("[]".to_string()),
            statics: Arc::new(StaticServer::new(
                std::path::Path::new("."),
                &GameConfig::default(),
            )),
            realtime: Arc::new(RealtimeHub::new(None)),
            sessions: Arc::new(SessionCache::new(60, 3600)),
            chain: Arc::new(std::sync::Mutex::new(
                crate::services::matchmake::chain::ChainTracker::default(),
            )),
            db: None,
            port: 0,
            status_rx: Arc::new(crate::app::state::OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            started: std::time::Instant::now(),
            dev_mode: false,
            bus: bus.clone(),
        })
    }
}
