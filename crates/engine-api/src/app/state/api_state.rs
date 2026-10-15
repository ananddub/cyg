//! The plane's handler state and the account-plane pool it carries.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use engine_bus::{Bus, StatusSnapshot};
use engine_config::{ControlConfig, GameConfig, SessionConfig};

use crate::infra::{rate_limit, seats, signaling};

use super::OnceReceiver;

/// The account plane's pool, or `None` when no DSN is configured. `connect_lazy` opens
/// no socket, so an unreachable Postgres still boots and the first `/v2` query reports
/// the failure instead of startup aborting.
fn connect_pool(session: &SessionConfig) -> Option<sqlx::PgPool> {
    if session.database_url.is_empty() {
        return None;
    }
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(session.max_db_connections.max(1))
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_lazy(&session.database_url)
        .ok()
}

/// Everything a handler needs. Cheap to clone (`Arc` inside), which is how it
/// reaches every Axum task without a lock on the API itself.
#[derive(Clone)]
pub struct ApiState {
    pub control: Arc<ControlConfig>,
    pub game: Arc<GameConfig>,
    pub bus: Arc<Bus>,
    pub seats: Arc<std::sync::Mutex<seats::SeatRegistry>>,
    pub limiter: Arc<std::sync::Mutex<rate_limit::IpRateLimiter>>,
    pub signals: signaling::SignalHub,
    pub statics: Arc<crate::services::static_routes::StaticServer>,
    /// The port the plane actually bound; replies carry it, so a caller that asked for
    /// port `0` still tells clients the truth.
    pub port: u16,
    /// Consumed by exactly one task ([`crate::infra::pump`]), which writes the latest
    /// sample into `latest_status`.
    pub status_rx: Arc<OnceReceiver>,
    pub latest_status: Arc<std::sync::Mutex<Option<StatusSnapshot>>>,
    pub started: Instant,
    /// The BaaS-compatibility database (`/v2/*` and `/ws`), connected lazily from
    /// `[control.session].database_url`. The game plane must boot with Postgres down, so
    /// every `/v2` handler turns `None` into a 503 rather than panicking.
    pub db: Option<sqlx::PgPool>,
    /// FlashORM-generated query wrapper over `db`, if the pool exists.
    pub q: Option<Arc<crate::repositories::db::Queries>>,
    /// Revoked session tokens, checked by every [`Authed`](crate::app::middleware::Authed)
    /// extraction. Shared, because a logout in one task must fail the same token in another.
    pub sessions: Arc<crate::infra::session_cache::SessionCache>,
}

impl ApiState {
    /// Build the handler state for a plane bound to `port`. The listener must already
    /// exist: replies carry the real port, and `api_port: 0` means "ask the kernel".
    pub fn new(
        project_dir: &Path,
        game: &GameConfig,
        control: &ControlConfig,
        bus: Arc<Bus>,
        port: u16,
    ) -> ApiState {
        let db = connect_pool(&control.session);
        let q = db
            .clone()
            .map(|pool| Arc::new(crate::repositories::db::Queries::new(pool)));
        // One plane per process, so the signing keys live in a process-wide constant.
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
            statics: Arc::new(crate::services::static_routes::StaticServer::new(project_dir, game)),
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            port,
            bus,
            started: Instant::now(),
            db,
            q,
            sessions,
        }
    }

    pub fn control_port(&self) -> u16 {
        self.port
    }

    /// Turn a seat token into the [`seats::Seat`] it names, or a human-readable reason —
    /// used by the signaling upgrade before the socket exists.
    pub fn verify_seat(&self, token: &str) -> Result<seats::Seat, String> {
        self.seats
            .lock()
            .map_err(|_| "seat registry is unavailable".to_string())?
            .verify(token)
            .ok_or_else(|| "seat token is invalid or expired".to_string())
    }
}
