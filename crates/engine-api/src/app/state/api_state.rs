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
            tracing::info!("[database] {} ({}/{})", session.database_endpoint, session.database_ns, session.database_db);
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
    pub limiter: Arc<std::sync::Mutex<rate_limit::IpRateLimiter>>,
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
        ApiState {
            control: Arc::new(control.clone()),
            game: Arc::new(game.clone()),
            seats: Arc::new(std::sync::Mutex::new(seats::SeatRegistry::new(
                &control.seat_secret,
            ))),
            limiter: Arc::new(std::sync::Mutex::new(rate_limit::IpRateLimiter::new(
                control.rate_limit_rps,
                control.rate_limit_burst,
            ))),
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
