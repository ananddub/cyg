//! Outbound HTTP with the stability patterns that keep a slow vendor from
//! becoming a slow plane.
//!
//! Every call this process makes to something it does not own — a store's
//! purchase receipt, a social provider's tokeninfo, a push gateway — goes
//! through [`send`]. Two things happen there that a bare `reqwest` call cannot
//! give us:
//!
//! * a **circuit breaker** per destination. A destination that has failed
//!   `http_breaker_trips` times in a row is left alone for a cooldown that
//!   doubles each time it fails again. While it is open no connection is even
//!   attempted, so a dead provider costs one in-memory check instead of a
//!   timeout per request.
//! * a **bulkhead** per destination. A destination we are waiting on may only
//!   hold `http_bulkhead` concurrent requests; the rest are refused immediately
//!   rather than queueing behind the slow one and taking the whole plane with
//!   them.
//!
//! Both are per destination, never global: one dead push gateway must not stop
//! purchase verification.

use std::sync::OnceLock;

use scc::HashMap;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use reqwest::{RequestBuilder, Response};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::infra::liveops;

/// Why a call never reached the destination, or failed while talking to it.
#[derive(Debug)]
pub enum HttpError {
    /// The breaker is open for this destination. Nothing was attempted.
    Open { retry_after_secs: u64 },
    /// The bulkhead is full: this destination already has its allowance in
    /// flight and this call would only queue behind it.
    Busy,
    /// The call was attempted and failed at the transport layer.
    Transport(reqwest::Error),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Open { retry_after_secs } => write!(
                formatter,
                "this provider is temporarily unavailable; retry in {retry_after_secs}s"
            ),
            HttpError::Busy => {
                write!(formatter, "too many calls to this provider are in flight")
            }
            HttpError::Transport(err) => write!(formatter, "{err}"),
        }
    }
}

impl From<HttpError> for crate::error::V2Error {
    fn from(err: HttpError) -> Self {
        match err {
            HttpError::Open { retry_after_secs } => {
                crate::error::V2Error::unavailable(err.to_string())
                    .with_retry_after(u32::try_from(retry_after_secs).unwrap_or(1).max(1))
            }
            HttpError::Busy => {
                crate::error::V2Error::unavailable(err.to_string()).with_retry_after(1)
            }
            HttpError::Transport(err) => {
                crate::error::V2Error::unavailable(format!("provider call failed: {err}"))
            }
        }
    }
}

/// What a breaker remembers about one destination.
struct Breaker {
    consecutive_failures: u32,
    /// `None` while closed. `Some(until)` while open: the instant the next probe
    /// is allowed.
    open_until: Option<Instant>,
    /// The current cooldown, doubled each time the half-open probe fails.
    cooldown: Duration,
    /// Whether the single half-open probe is already out. While it is, further
    /// calls are refused so a burst cannot all race the same recovery probe.
    probe_out: bool,
}

impl Breaker {
    fn new() -> Breaker {
        Breaker {
            consecutive_failures: 0,
            open_until: None,
            cooldown: Duration::from_millis(liveops::config().http_breaker_cooldown_ms),
            probe_out: false,
        }
    }

    fn state(&self, now: Instant) -> BreakerState {
        match self.open_until {
            None => BreakerState::Closed,
            Some(until) if now >= until => BreakerState::HalfOpen,
            Some(_) => BreakerState::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    HalfOpen,
    Open,
}

impl BreakerState {
    /// The value on the Prometheus gauge: 0 closed, 1 half-open, 2 open.
    pub fn gauge(self) -> u8 {
        match self {
            BreakerState::Closed => 0,
            BreakerState::HalfOpen => 1,
            BreakerState::Open => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BreakerState::Closed => "closed",
            BreakerState::HalfOpen => "half_open",
            BreakerState::Open => "open",
        }
    }
}

struct Plane {
    breakers: HashMap<String, Breaker>,
    bulkheads: HashMap<String, std::sync::Arc<Semaphore>>,
}

fn plane() -> &'static Plane {
    static PLANE: OnceLock<Plane> = OnceLock::new();
    PLANE.get_or_init(|| Plane {
        breakers: HashMap::new(),
        bulkheads: HashMap::new(),
    })
}

fn bulkhead(destination: &str) -> std::sync::Arc<Semaphore> {
    plane()
        .bulkheads
        .entry_sync(destination.to_owned())
        .or_insert_with(|| {
            std::sync::Arc::new(Semaphore::new(liveops::config().http_bulkhead.max(1)))
        })
        .get()
        .clone()
}

/// Asks the breaker to let one call out. A half-open breaker lets exactly one
/// probe through; the rest are refused until that probe reports back.
fn admit(destination: &str) -> Result<(), HttpError> {
    let mut entry = plane()
        .breakers
        .entry_sync(destination.to_owned())
        .or_insert_with(Breaker::new);
    let breaker = entry.get_mut();
    let now = Instant::now();
    match breaker.state(now) {
        BreakerState::Closed => Ok(()),
        BreakerState::Open => Err(HttpError::Open {
            retry_after_secs: breaker
                .open_until
                .map(|until| until.saturating_duration_since(now).as_secs().max(1))
                .unwrap_or(1),
        }),
        BreakerState::HalfOpen if breaker.probe_out => Err(HttpError::Open {
            retry_after_secs: breaker.cooldown.as_secs().max(1),
        }),
        BreakerState::HalfOpen => {
            breaker.probe_out = true;
            Ok(())
        }
    }
}

/// Records what one call said about its destination. Only a transport failure or
/// a 5xx counts against the breaker: a 4xx is the provider answering, which
/// proves it is alive.
fn record(destination: &str, outcome: Outcome) {
    let mut entry = plane()
        .breakers
        .entry_sync(destination.to_owned())
        .or_insert_with(Breaker::new);
    let breaker = entry.get_mut();
    let config = liveops::config();
    match outcome {
        Outcome::Alive => {
            breaker.consecutive_failures = 0;
            breaker.open_until = None;
            breaker.probe_out = false;
            breaker.cooldown = Duration::from_millis(config.http_breaker_cooldown_ms);
        }
        Outcome::Failed => {
            breaker.probe_out = false;
            breaker.consecutive_failures += 1;
            let tripped = breaker.state(Instant::now()) == BreakerState::HalfOpen
                || breaker.consecutive_failures >= config.http_breaker_trips.max(1) as u32;
            if tripped {
                breaker.open_until = Some(Instant::now() + breaker.cooldown);
                // The next outage is given more room, up to the cap, so a
                // destination that never recovers is probed once every few
                // minutes instead of once every cooldown forever.
                let max = Duration::from_millis(config.http_breaker_max_cooldown_ms);
                breaker.cooldown = (breaker.cooldown * 2).min(max);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Alive,
    Failed,
}

/// Sends one request through the destination's breaker and bulkhead.
///
/// `Ok` means the destination answered, whatever the status: the caller decides
/// what a 4xx means. `Err` means it was not asked, or could not be reached.
pub async fn send(destination: &str, request: RequestBuilder) -> Result<Response, HttpError> {
    // The bulkhead is checked first so our own overload can never eat the
    // half-open probe: a refused permit leaves the breaker exactly as it was.
    let permit: OwnedSemaphorePermit = bulkhead(destination)
        .try_acquire_owned()
        .map_err(|_| HttpError::Busy)?;
    admit(destination)?;
    let response = request.send().await;
    drop(permit);
    match response {
        Ok(response) => {
            if response.status().is_server_error()
                || response.status() == StatusCode::TOO_MANY_REQUESTS
            {
                record(destination, Outcome::Failed);
            } else {
                record(destination, Outcome::Alive);
            }
            Ok(response)
        }
        Err(err) => {
            record(destination, Outcome::Failed);
            Err(HttpError::Transport(err))
        }
    }
}

/// One line per destination for the Prometheus endpoint.
pub fn snapshot() -> Vec<(&'static str, String)> {
    let now = Instant::now();
    let mut rows: Vec<(String, BreakerState, u32)> = Vec::new();
    plane().breakers.scan_sync(|destination, breaker| {
        rows.push((
            destination.clone(),
            breaker.state(now),
            breaker.consecutive_failures,
        ));
        true
    });
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (destination, state, failures) in rows {
        out.push((
            "cyg_http_breaker_state",
            format!(
                "cyg_http_breaker_state{{destination=\"{destination}\"}} {}",
                state.gauge()
            ),
        ));
        out.push((
            "cyg_http_breaker_failures",
            format!("cyg_http_breaker_failures{{destination=\"{destination}\"}} {failures}"),
        ));
    }
    out
}

#[cfg(test)]
mod tests;
