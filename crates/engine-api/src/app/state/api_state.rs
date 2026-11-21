use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use engine_bus::{Bus, StatusSnapshot};
use engine_config::{ControlConfig, GameConfig, SessionConfig};

use crate::db::{BlobStore, Datastore, KvBudget};
use crate::infra::realtime::RealtimeHub;
use crate::infra::{rate_limit, seats, signaling};

use super::OnceReceiver;

async fn connect_datastore(session: &SessionConfig) -> Option<Arc<Datastore>> {
    if session.database_endpoint.is_empty() {
        return None;
    }
    let user = (!session.database_user.is_empty()).then_some(session.database_user.as_str());
    let pass = (!session.database_pass.is_empty()).then_some(session.database_pass.as_str());
    let budget = KvBudget {
        memtable_mib: (session.database_memtable_mib > 0).then_some(session.database_memtable_mib),
        cache_mib: (session.database_cache_mib > 0).then_some(session.database_cache_mib),
    };
    match Datastore::open(
        &session.database_endpoint,
        &session.database_ns,
        &session.database_db,
        user,
        pass,
        budget,
    )
    .await
    {
        Ok(db) => {
            tracing::info!(
                "[database] {} ({}/{})",
                session.database_endpoint,
                session.database_ns,
                session.database_db
            );
            Some(Arc::new(db))
        }
        Err(err) => {
            tracing::error!("[database] {}: {err}", session.database_endpoint);
            None
        }
    }
}

async fn connect_blobs(
    session: &SessionConfig,
    db: Option<Arc<Datastore>>,
) -> Option<Arc<BlobStore>> {
    let db = db?;
    match BlobStore::open(&session.blobs, &session.database_endpoint, Some(db)).await {
        Ok(store) => Some(Arc::new(store)),
        Err(err) => {
            tracing::error!("[blobs] {err}");
            None
        }
    }
}

#[derive(Clone)]
pub struct ApiState {
    pub control: Arc<ControlConfig>,
    pub game: Arc<GameConfig>,
    pub bus: Arc<Bus>,
    pub seats: Arc<std::sync::Mutex<seats::SeatRegistry>>,
    /// Sharded per-IP rate limiter: locking happens per shard inside the
    /// limiter, so concurrent matchmake calls from different addresses do not
    /// serialize on one mutex.
    pub limiter: Arc<rate_limit::IpRateLimiter>,
    pub signals: signaling::SignalHub,
    pub statics: Arc<crate::services::static_routes::StaticServer>,
    pub port: u16,
    pub status_rx: Arc<OnceReceiver>,
    pub latest_status: Arc<std::sync::Mutex<Option<StatusSnapshot>>>,
    pub started: Instant,
    pub db: Option<Arc<Datastore>>,
    pub q: Option<Arc<crate::repositories::db::Queries>>,
    pub realtime: Arc<RealtimeHub>,
    pub blobs: Option<Arc<BlobStore>>,
    pub sessions: Arc<crate::infra::session_cache::SessionCache>,
    /// Outstanding variant seats per room key — matchmaking chaining counts
    /// these on top of the host's live occupancy so a burst of joins within
    /// one status interval cannot all land in the same full room.
    pub chain: Arc<std::sync::Mutex<crate::services::matchmake::chain::ChainTracker>>,
    /// Customer logic that answers account events, loaded once at boot.
    ///
    /// `None` when the project ships no hooks module, which is the common case —
    /// every call site treats that as "no hook answered", not as an error.
    pub hooks: Option<Arc<engine_hooks::HookRunner>>,
    /// Whether this is an explicitly-started local development session
    /// (`cyg run -d`, see `engine_control::runtime_mode`). Resolved once at
    /// boot so no request-time code reads the environment; per-request the
    /// grant still requires a loopback source address (see
    /// `middleware::dev_admin`).
    pub dev_mode: bool,
}

impl ApiState {
    pub async fn new(
        project_dir: &Path,
        game: &GameConfig,
        control: &ControlConfig,
        bus: Arc<Bus>,
        port: u16,
    ) -> ApiState {
        let db = connect_datastore(&control.session).await;
        let q = db
            .clone()
            .map(|db| Arc::new(crate::repositories::db::Queries::new(db)));
        let blobs = connect_blobs(&control.session, db.clone()).await;
        let realtime = Arc::new(RealtimeHub::new(db.clone()));
        crate::infra::jwt::configure(&control.session);
        crate::infra::console::configure(&control.console);
        let sessions = Arc::new(crate::infra::session_cache::SessionCache::new(
            control.session.token_expiry_secs,
            control.session.refresh_expiry_secs,
        ));
        let chain = Arc::new(std::sync::Mutex::new(
            crate::services::matchmake::chain::ChainTracker::default(),
        ));
        // Loaded once, here, because the runner owns a compiled module: compiling
        // it per request would cost more than the hook it runs. The server loads
        // its own copy for room boot; both read the same file.
        let hooks = engine_hooks::boot_hooks(project_dir, game).map(Arc::new);
        // The per-account bucket stays off with the quotas, so a plane that has
        // not opted in cannot start refusing authenticated traffic.
        let limiter = rate_limit::IpRateLimiter::new(control.rate_limit_rps, control.rate_limit_burst);
        if control.quotas.enabled {
            limiter.set_player_rates(control.quotas.player_rps, control.quotas.player_burst);
        }
        let limiter = Arc::new(limiter);
        // Read once here: per-request environment lookups would let a change
        // after boot silently widen the dev admin grant.
        let dev_mode = std::env::var("CYG_DEV_MODE").as_deref() == Ok("1");
        ApiState {
            control: Arc::new(control.clone()),
            game: Arc::new(game.clone()),
            chain,
            seats: Arc::new(std::sync::Mutex::new(seats::SeatRegistry::new(
                &control.seat_secret,
            ))),
            limiter,
            signals: signaling::SignalHub::new(game.voice_ice_servers.clone()),
            statics: Arc::new(crate::services::static_routes::StaticServer::new(
                project_dir,
                game,
            )),
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            port,
            bus,
            started: Instant::now(),
            db,
            q,
            blobs,
            realtime,
            sessions,
            hooks,
            dev_mode,
        }
    }

    pub fn control_port(&self) -> u16 {
        self.port
    }

    pub fn verify_seat(&self, token: &str) -> Result<seats::Seat, String> {
        self.seats
            .lock()
            .map_err(|_| "seat registry is unavailable".to_string())?
            .verify(token)
            .ok_or_else(|| "seat token is invalid or expired".to_string())
    }
}
