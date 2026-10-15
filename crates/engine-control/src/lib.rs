//! # engine-control — Tokio control plane
//!
//! Extracted from the root binary's `src/control/` and `src/threads/status.rs`.
//! Owns matchmaking, HTTP/WS signaling, static website routes, seat tokens,
//! rate limiting, Agones lifecycle, and status reporting.

pub mod agones;
pub mod dashboard;
pub mod dashboard_assets;
pub mod dashboard_ws;
pub mod matchmake;
pub mod rate_limit;
pub mod seats;
pub mod signaling;
pub mod static_routes;
pub mod status;

// # Control plane (Agent 1) — Tokio/Axum
//
// Everything browser-facing that is *not* the game simulation: static assets,
// `matchmake`, the WebSocket signaling relay, `/status`, and the Agones
// lifecycle. It runs on its own Tokio runtime on its own cores and touches
// room state only by sending on [`engine_bus::Bus`].
//
// Layout — each file below is owned by exactly one workstream and exposes one
// frozen entry point. `lib.rs` is the only place routers are composed, so no
// workstream ever needs to edit another's file to add a route:
//
// * [`static_routes`] — `pub fn router(&ControlState) -> Router<ControlState>`
// * [`matchmake`]     — same signature
// * [`signaling`]     — same signature, plus the [`signaling::SignalHub`] seam
// * [`status`]        — same signature
// * [`seats`], [`rate_limit`] — types only, used by `matchmake`
//
// ## Port split
//
// The game WebSocket stays on `cfg.port` inside `engine-net`'s transport; this
// plane binds `cfg.control.api_port` (falling back to `web_port`). Two
// listeners, one port each, so a slow HTTP handler can stall neither the
// 128 Hz tick nor the WS accept loop.

use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use crossbeam_channel::Receiver;

use engine_bus::{Bus, StatusSnapshot};
use engine_config::{ControlConfig, GameConfig};

/// Everything a handler needs. Cheap to clone (`Arc` inside), which is how it
/// reaches every Axum task without a lock on the plane itself.
#[derive(Clone)]
pub struct ControlState {
    pub control: Arc<ControlConfig>,
    pub game: Arc<GameConfig>,
    pub bus: Arc<Bus>,
    pub seats: Arc<std::sync::Mutex<seats::SeatRegistry>>,
    pub limiter: Arc<std::sync::Mutex<rate_limit::IpRateLimiter>>,
    pub signals: signaling::SignalHub,
    pub statics: Arc<static_routes::StaticServer>,
    /// The port this plane actually bound. Replies carry it, so a caller that
    /// asked for port `0` still tells clients the truth.
    pub port: u16,
    /// Consumed by exactly one task ( [`status::pump`] ), which writes the
    /// latest sample into `latest_status`. Held here so `Bus` stays the bus
    /// owner's file.
    pub status_rx: Arc<OnceReceiver>,
    pub latest_status: Arc<std::sync::Mutex<Option<StatusSnapshot>>>,
    pub started: Instant,
}

/// A receiver that can be handed to the router but must not be duplicated: two
/// consumers of `bus.status_rx` would each see half the samples.
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

/// A running control plane. Keep this alive: dropping it shuts the Tokio
/// runtime down, and with it every HTTP and signaling connection.
pub struct ControlPlane {
    runtime: tokio::runtime::Runtime,
    /// The bind port the kernel actually handed us, so a caller asking for
    /// port `0` in a test can find out where it landed.
    pub port: u16,
}

impl ControlState {
    /// The port clients must reach this plane on.
    pub fn control_port(&self) -> u16 {
        self.port
    }

    /// Turn a seat token into the [`Seat`] it names, or a human-readable reason.
    ///
    /// Used by the signaling upgrade (before the socket exists) and by any
    /// future route that needs a room + expiry from an untrusted string. The
    /// registry also records the mint, so a token signed with this plane's
    /// secret but never handed out is still accepted — see
    /// [`seats::SeatRegistry::verify`].
    pub fn verify_seat(&self, token: &str) -> Result<seats::Seat, String> {
        self.seats
            .lock()
            .map_err(|_| "seat registry is unavailable".to_string())?
            .verify(token)
            .ok_or_else(|| "seat token is invalid or expired".to_string())
    }
}

impl ControlPlane {
    /// Build the router: state, then the four route groups, then the catch-all.
    fn build_router(state: ControlState) -> Router {
        Router::new()
            .merge(static_routes::router(&state))
            .merge(matchmake::router(&state))
            .merge(signaling::router(&state))
            .merge(status::router(&state))
            .merge(dashboard::router(&state))
            .merge(dashboard_ws::router(&state))
            .merge(dashboard_assets::router(&state))
            // One fallback for the whole plane: anything unmatched is a static
            // file request, and `StaticServer` decides whether it exists.
            .fallback(static_routes::fallback)
            .with_state(state)
    }

    /// Stop the plane and shut connections down cleanly. Bounded because a
    /// hung client must not turn Ctrl-C into a hang.
    pub fn shutdown(self) {
        self.runtime.shutdown_timeout(Duration::from_secs(2));
    }
}

/// Start the control plane on its own multi-thread Tokio runtime.
///
/// Returns once the listener is bound, so the caller can print a real port.
/// Nothing here blocks the simulation thread: the plane only ever `try_send`s
/// onto the bus.
pub fn spawn(
    project_dir: &Path,
    game: &GameConfig,
    control: &ControlConfig,
    bus: Arc<Bus>,
) -> std::io::Result<ControlPlane> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(control.workers.max(1))
        .thread_name("ox-control")
        .enable_all()
        .build()?;

    let ice_servers = game.voice_ice_servers.clone();

    // Bind before building the state: the seat reservation and `/status` both
    // carry the real port, and `api_port: 0` means "ask the kernel".
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind((control.bind_addr.as_str(), control.api_port)))?;
    let port = listener.local_addr()?.port();

    let state = ControlState {
        control: Arc::new(control.clone()),
        game: Arc::new(game.clone()),
        bus: bus.clone(),
        seats: Arc::new(std::sync::Mutex::new(seats::SeatRegistry::new(
            &control.seat_secret,
        ))),
        limiter: Arc::new(std::sync::Mutex::new(rate_limit::IpRateLimiter::new(
            control.rate_limit_rps,
            control.rate_limit_burst,
        ))),
        signals: signaling::SignalHub::new(ice_servers),
        statics: Arc::new(static_routes::StaticServer::new(project_dir, game)),
        port,
        status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(std::sync::Mutex::new(None)),
        started: Instant::now(),
    };

    let router = ControlPlane::build_router(state.clone());

    runtime.spawn(async move {
        // `into_make_service_with_connect_info` is what makes the peer socket
        // address reachable as `ConnectInfo<SocketAddr>`; without it every
        // matchmake request fails on a missing request extension and the IP
        // bucket is never charged.
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_signal())
            .await;
    });

    // One consumer of the bus status queue, so `/status` always has the newest
    // sample without touching the simulation thread's lock-free path.
    runtime.spawn(status::pump(state.clone()));

    // Expiring seats must not accumulate if nothing ever calls `verify`.
    runtime.spawn(gc_seats(state.clone()));

    // Agones sidecar lifecycle: Ready, then the Health ping loop. No-op unless
    // `[control.agones] enabled = true`.
    agones::spawn(&state);

    // Ctrl-C is a pod's last words. Stopping the beat and telling the sidecar to
    // retire this GameServer is what keeps the agent from holding a dead pod in
    // the Ready pool until the health timeout — and the timeout is how long
    // clients keep being matched into a server that no longer answers.
    if control.agones.enabled {
        runtime.spawn(async {
            shutdown_signal().await;
            match agones::shutdown() {
                Ok(sent) => eprintln!(
                    "[agones] 🛑 {} shutdown sent; this pod leaves the pool",
                    if sent { "first" } else { "repeat" }
                ),
                Err(e) => eprintln!("[agones] ✖ could not retire the pod: {e}"),
            }
        });
    }

    Ok(ControlPlane { runtime, port })
}

/// Seats are minted faster than they are used (abandoned page loads). Sweep
/// every minute; the lock is held for a vec retain, not an HMAC verify.
async fn gc_seats(state: ControlState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Ok(mut registry) = state.seats.lock() {
            registry.retire_expired();
        }
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// A handler's failure: one status code plus a JSON `{"error": …}` body, so a
/// browser can tell *why* a request was refused instead of guessing.
pub struct ApiError {
    pub status: StatusCode,
    pub message: Cow<'static, str>,
    pub retry_after: Option<u32>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> ApiError {
        ApiError { status, message: message.into(), retry_after: None }
    }
    /// 429 with the `Retry-After` the bucket computed, so a well-behaved page
    /// backs off instead of retrying in a tight loop.
    pub fn rate_limited(retry_after_secs: u32) -> ApiError {
        ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: Cow::Borrowed("matchmake rate limit exceeded"),
            retry_after: Some(retry_after_secs.max(1)),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Serialised, not `{:?}`: a debug-escape is not JSON, so a message with a
        // quote or a non-ASCII character in it (a room name echoed back, a
        // translated string) would reach the browser as `\"` and `\u…` text.
        let body = serde_json::to_string(&serde_json::json!({ "error": self.message }))
            .unwrap_or_else(|_| "{\"error\":\"internal\"}".to_string());
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().expect("static header value"),
        );
        if let Some(secs) = self.retry_after {
            headers.insert(
                axum::http::header::RETRY_AFTER,
                secs.to_string().parse().expect("seconds are a valid header"),
            );
        }
        (self.status, headers, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_of(err: ApiError) -> (StatusCode, String, Option<String>) {
        let response = err.into_response();
        let status = response.status();
        let retry = response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("error body");
        (status, String::from_utf8_lossy(&bytes).into_owned(), retry)
    }

    /// A refusal has to be readable, and the message often quotes text the
    /// caller sent — a room name or a method segment. Debug-escaping that into
    /// JSON is how a browser ends up parsing `"rooms\u00e9"`.
    #[tokio::test]
    async fn an_error_body_is_json_even_when_the_message_is_not_ascii() {
        let (status, body, retry) = body_of(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid room name: réssort\"x\"",
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(retry.is_none(), "a plain error advertises no backoff");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
        assert_eq!(parsed["error"], "invalid room name: réssort\"x\"");
    }

    #[tokio::test]
    async fn a_rate_limit_error_carries_a_usable_retry_after() {
        let (status, body, retry) = body_of(ApiError::rate_limited(0)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(retry.as_deref(), Some("1"), "zero seconds would tell a client to hammer again");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("{error} is JSON");
        assert!(parsed["error"].is_string());
    }
}
