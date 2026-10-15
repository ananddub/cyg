//! The shared handler state and the single-take status receiver.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Receiver;

use engine_bus::{Bus, StatusSnapshot};
use engine_config::{ControlConfig, GameConfig};

use crate::{rate_limit, seats, signaling, static_routes};

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
    pub statics: Arc<static_routes::StaticServer>,
    /// The port the plane actually bound. Replies carry it, so a caller that
    /// asked for port `0` still tells clients the truth.
    pub port: u16,
    /// Consumed by exactly one task (crate::status::pump), which writes the
    /// latest sample into `latest_status`.
    pub status_rx: Arc<OnceReceiver>,
    pub latest_status: Arc<std::sync::Mutex<Option<StatusSnapshot>>>,
    pub started: Instant,
}

impl ApiState {
    /// Build the handler state for a plane bound to `port`. The listener must
    /// already exist: replies carry the real port, and `api_port: 0` means
    /// "ask the kernel".
    pub fn new(
        project_dir: &Path,
        game: &GameConfig,
        control: &ControlConfig,
        bus: Arc<Bus>,
        port: u16,
    ) -> ApiState {
        ApiState {
            control: Arc::new(control.clone()),
            game: Arc::new(game.clone()),
            seats: Arc::new(std::sync::Mutex::new(seats::SeatRegistry::new(&control.seat_secret))),
            limiter: Arc::new(std::sync::Mutex::new(rate_limit::IpRateLimiter::new(
                control.rate_limit_rps,
                control.rate_limit_burst,
            ))),
            signals: signaling::SignalHub::new(game.voice_ice_servers.clone()),
            statics: Arc::new(static_routes::StaticServer::new(project_dir, game)),
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            port,
            bus,
            started: Instant::now(),
        }
    }

    /// The port clients must reach this plane on.
    pub fn control_port(&self) -> u16 {
        self.port
    }

    /// Turn a seat token into the [`seats::Seat`] it names, or a
    /// human-readable reason. Used by the signaling upgrade, before the
    /// socket exists.
    pub fn verify_seat(&self, token: &str) -> Result<seats::Seat, String> {
        self.seats
            .lock()
            .map_err(|_| "seat registry is unavailable".to_string())?
            .verify(token)
            .ok_or_else(|| "seat token is invalid or expired".to_string())
    }
}

/// A receiver that can be handed to the router but must not be duplicated:
/// two consumers of `bus.status_rx` would each see half the samples.
#[derive(Default)]
pub struct OnceReceiver {
    inner: std::sync::Mutex<Option<Receiver<StatusSnapshot>>>,
}

impl OnceReceiver {
    pub fn new(rx: Receiver<StatusSnapshot>) -> OnceReceiver {
        OnceReceiver { inner: std::sync::Mutex::new(Some(rx)) }
    }
    /// `None` once taken — the pump stops and `/status` keeps reporting the
    /// last sample it saw rather than failing.
    pub fn try_take(&self) -> Option<Receiver<StatusSnapshot>> {
        self.inner.lock().ok()?.take()
    }
}
