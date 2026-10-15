//! Status: the HTTP telemetry routes (`/status`, `/api/rooms`) plus the
//! hot-path latency recorder that feeds them.
//!
//! This module is the merge of the root binary's `src/control/status.rs`
//! (browser-facing telemetry on the Tokio plane) and `src/threads/status.rs`
//! (the sim thread's `Recorder`). Their items did not collide, so no renaming
//! was needed — both live here verbatim.
//!
//! The sim plane publishes a [`StatusSnapshot`] about once a second onto
//! `bus.status_tx` (via [`Recorder::maybe_publish`]). Exactly one task on this
//! plane — [`pump`] — consumes that queue and keeps the newest sample in a
//! mutex, so `/status` never races a second reader and never touches the sim
//! thread.
//!
//! `updated_ms` in the snapshot is *monotonic* milliseconds from the same
//! process-wide epoch as [`mono_ns`], which is what makes `age_ms` here
//! meaningful without involving wall-clock skew.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use crate::ControlState;
use engine_bus::{Bus, StatusSnapshot};

// ---- helpers duplicated from plane (src/threads/mod.rs) --------------------
//
// engine-control must not depend on engine-server, so the two tiny clock
// helpers the status code needs are re-declared here with identical bodies.

/// Monotonic nanoseconds since an arbitrary process-wide epoch.
/// // duplicated from plane
pub fn mono_ns() -> u64 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    EPOCH.elapsed().as_nanos() as u64
}

/// Nominal tick period for `tick_hz`.
/// // duplicated from plane
pub fn tick_period(tick_hz: f64) -> Duration {
    Duration::from_nanos((1e9 / tick_hz.max(1.0)) as u64)
}

// ---- the HTTP routes (moved from src/control/status.rs) --------------------

/// How long the pump sleeps between drains. The sim publishes once a second,
/// so this only bounds how stale a sample can look after the sender wakes up.
const PUMP_INTERVAL: Duration = Duration::from_millis(250);

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new()
        .route("/status", get(status))
        .route("/api/rooms", get(rooms))
}

/// The one consumer of `bus.status_rx`.
///
/// Returns immediately if someone else already took the queue: `/status` then
/// keeps reporting the last sample it ever saw instead of failing.
pub async fn pump(state: ControlState) {
    let Some(rx) = state.status_rx.try_take() else {
        eprintln!("[control] status queue already owned; /status serves the last sample");
        return;
    };
    // `crossbeam_channel::Receiver` is not awaitable, so this polls on a
    // quarter-second tick. The sim publishes once a second; polling only decides
    // how quickly a fresh sample becomes visible, never whether it is lost.
    let mut ticker = tokio::time::interval(PUMP_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        // Drain everything queued and keep the newest: a plane that slept for
        // 5 s must not report a 5 s-old sample as current.
        let mut newest = None;
        while let Ok(snap) = rx.try_recv() {
            newest = Some(snap);
        }
        if let Some(snap) = newest
            && let Ok(mut slot) = state.latest_status.lock()
        {
            crate::dashboard::record_sample(snap.clone());
            *slot = Some(snap);
        }
    }
}

/// `age_ms` against the sim's own monotonic clock.
fn age_ms(snap: &StatusSnapshot) -> u64 {
    let now = mono_ns() / 1_000_000;
    now.saturating_sub(snap.updated_ms)
}

async fn status(State(state): State<ControlState>) -> Response {
    let snapshot = state
        .latest_status
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .map(|snap| {
            let mut obj = serde_json::to_value(&snap).unwrap_or_else(|_| json!({}));
            if let Some(map) = obj.as_object_mut() {
                map.insert("age_ms".to_string(), json!(age_ms(&snap)));
            }
            obj
        });

    let (seats_minted, seats_rejected) = state
        .seats
        .lock()
        .map(|r| r.counts())
        .unwrap_or((0, 0));
    // Mint refusals and the registry's bound: `outstanding == bound` with
    // `refused` climbing is a lobby full of abandoned page loads, and the only
    // thing an operator can act on.
    let (seats_refused, seats_bound) = state
        .seats
        .lock()
        .map(|r| (r.refused(), r.seat_bound()))
        .unwrap_or((0, 0));
    let (limit_allowed, limit_denied) = state
        .limiter
        .lock()
        .map(|l| l.counts())
        .unwrap_or((0, 0));
    // Refusals from the tracked-source ceiling are a different failure from an
    // empty bucket: churn of source addresses, not a client sending too fast.
    let (limit_squeezed, limit_swept) = state
        .limiter
        .lock()
        .map(|l| (l.squeezed(), l.swept()))
        .unwrap_or((0, 0));
    let (signals_delivered, signals_dropped) = state.signals.counts();

    Json(json!({
        "plane": {
            "uptime_ms": state.started.elapsed().as_millis() as u64,
            "controlPort": state.control_port(),
            "workers": state.control.workers,
            "game": { "name": state.game.name, "port": state.game.port, "webPort": state.game.web_port },
        },
        "sim": snapshot,
        "seats": {
            "outstanding": state.seats.lock().map(|r| r.outstanding()).unwrap_or(0),
            "minted": seats_minted,
            "rejected": seats_rejected,
            "refused": seats_refused,
            "bound": seats_bound,
            "ttlSecs": state.control.seat_ttl_secs,
        },
        "rateLimit": {
            "allowed": limit_allowed,
            "denied": limit_denied,
            "squeezed": limit_squeezed,
            "swept": limit_swept,
            "tracked": state.limiter.lock().map(|l| l.tracked()).unwrap_or(0),
            "rps": state.control.rate_limit_rps,
            "burst": state.control.rate_limit_burst,
        },
        "signaling": {
            "peers": state.signals.peers(),
            "delivered": signals_delivered,
            "droppedFull": signals_dropped,
        },
        // The Agones sidecar's own last-known state. `enabled:false` is the
        // honest answer outside Kubernetes, which is how an operator tells "no
        // sidecar configured" from "sidecar configured and refusing us".
        "agones": serde_json::to_value(crate::agones::status_snapshot())
            .unwrap_or_else(|_| json!({})),
        // Live queue depths on the bus. `len()` on crossbeam is an approximation
        // under concurrent sends, which is exactly what a dashboard wants to see:
        // a number that only ever grows means a consumer stopped.
        "bus": {
            "inputsQueued": state.bus.inputs_rx.len(),
            "commandsQueued": state.bus.commands_rx.len(),
            "deltasQueued": state.bus.deltas_rx.len(),
        },
    }))
    .into_response()
}

/// The host a client can actually dial.
///
/// `bind_addr` is what the *server* bound, and `0.0.0.0` is not a connectable
/// address; `api_port: 0` means the port may not be the configured one either.
/// So echo the authority the caller used, re-pinned to the port this plane
/// really holds, and fall back to loopback when no `Host` arrived.
fn client_host(headers: &axum::http::HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|authority| match authority.rsplit_once(':') {
            // Strip an existing port: a bare IPv6 host has none, and IPv6
            // literal ports follow a `]`, so this only fires for IPv4/names.
            Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && !h.contains(']') => h,
            _ => authority,
        })
        .unwrap_or("127.0.0.1");
    match host {
        "0.0.0.0" | "::" | "" | "*" => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}

/// The room list a dashboard or the SDK's `getAvailableRooms` reads. There is
/// exactly one room per process in this topology (`src/server.rs` owns the
/// wasm host), so this reports that room with live occupancy rather than
/// inventing a registry.
async fn rooms(State(state): State<ControlState>, headers: axum::http::HeaderMap) -> Response {
    let snapshot = state
        .latest_status
        .lock()
        .ok()
        .and_then(|g| g.clone());
    let players = snapshot.as_ref().map(|s| s.players).unwrap_or(0);
    let tick_hz = snapshot.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0);

    // `serverId` is what the Colyseus SDK echoes back into `joinById`; a seat
    // token is not a room instance id, so this is an address, not a handle.
    let host = client_host(&headers);
    // `updated_ms` is monotonic, so an absent sample also means the *age* is
    // unknown: report `null` rather than a plausible-looking zero.
    let age_ms = snapshot.as_ref().map(age_ms);
    Json(json!([{
        "name": state.game.name,
        "clients": players,
        "maxClients": state.game.max_clients,
        "ready": players < state.game.max_clients.max(0) as u32,
        "tickHz": tick_hz,
        // What the project asks for, so a client can tell an idle room from a
        // sim that is running slower than configured.
        "tickRate": state.game.tick_rate,
        "port": state.game.port,
        "controlPort": state.control_port(),
        "serverId": format!("http://{host}:{}", state.control_port()),
        // Port-qualified: a bare `ws://0.0.0.0/ws/signal` would send a browser
        // to port 80 and look like a dead server.
        "signalingUrl": format!("ws://{host}:{}/ws/signal", state.control_port()),
        "udpPort": std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into()),
        "hasSimSample": snapshot.is_some(),
        "ageMs": age_ms,
    }]))
    .into_response()
}

// ---- the sim-side recorder (moved from src/threads/status.rs) --------------

use engine_bus::{SessionTelemetry, TICK_HISTOGRAM_BUCKETS};
use engine_dashboard::{MAX_CLIENT_ROWS, histogram_add};

/// Samples kept per window: 1 s at ≤ 512 inputs/s or 128 ticks/s fits easily.
const RING: usize = 1024;

/// What the publishing loop contributes beyond its own timing windows: the
/// session facts only the loop that owns the sockets can know. A struct rather
/// than positional args because the host loop, the pinned sim plane and a test
/// each fill a different subset, and five adjacent `u32`s cannot show which.
#[derive(Clone, Debug, Default)]
pub struct PublishReport {
    /// Measured tick rate over the publish window.
    pub tick_hz_est: f64,
    /// The configured rate (`GameConfig::tick_rate`). 0 leaves
    /// `nominal_tick_us` at 0 rather than inventing a period.
    pub tick_rate_hz: u32,
    /// Room members, spectators excluded.
    pub players: u32,
    /// Sessions counted on the transport their packets actually arrive on, so
    /// the three sum to the connected clients rather than double-counting a
    /// browser socket that upgraded to a data channel.
    pub ws_sessions: u32,
    pub webrtc_sessions: u32,
    pub udp_sessions: u32,
    /// One row per live session; the recorder truncates it to `session_cap`.
    pub sessions: Vec<SessionTelemetry>,
    /// Ceiling on `sessions`, normally the room's `max_clients`. 0 means the
    /// contract ceiling [`MAX_CLIENT_ROWS`] alone.
    pub session_cap: usize,
}

pub struct Recorder {
    input_lat_ns: [u64; RING],
    tick_us: [u64; RING],
    n_input: usize,
    n_tick: usize,
    /// Tick durations of the current window, added by `note_tick` and rolled by
    /// `maybe_publish` exactly like the rings, so it always describes the
    /// second the reader is looking at.
    tick_hist: [u32; TICK_HISTOGRAM_BUCKETS],
    /// Armed when the session set empties, cleared the moment it is occupied
    /// again; the source of `empty_for_sec`.
    empty_since: Option<Instant>,
    pub deltas_sent: u64,
    pub deltas_dropped_full: u64,
    pub inputs_dropped: u64,
    pub tick_overruns: u64,
    last_publish: Instant,
    /// Nominal tick rate, refreshed by `maybe_publish`; used only to classify
    /// a tick duration as an overrun against `1/tick_hz` of work.
    tick_hz: f64,
    publish_seq: u64,
}

/// Push one sample into a circular ring; once full it overwrites the oldest.
/// `n` is a free-running count — the window is `min(n, RING)` entries.
fn ring_push(buf: &mut [u64; RING], n: &mut usize, sample: u64) {
    buf[*n % RING] = sample;
    *n = n.saturating_add(1);
}

fn window(buf: &[u64; RING], n: usize) -> Vec<u64> {
    let count = n.min(RING);
    let mut v = buf[..count].to_vec();
    v.sort_unstable();
    v
}

/// Nearest-rank percentile: rank = ceil(len * num / den), clamped into range.
fn percentile(sorted: &[u64], num: usize, den: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * num + den - 1) / den;
    sorted[(rank - 1).min(sorted.len() - 1)]
}

/// Nominal period of one tick at `tick_rate_hz`, microseconds. Unknown rates
/// report 0 rather than a plausible 128 Hz period.
fn nominal_tick_us(tick_rate_hz: u32) -> u64 {
    if tick_rate_hz == 0 {
        0
    } else {
        1_000_000 / tick_rate_hz as u64
    }
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder {
            input_lat_ns: [0; RING],
            tick_us: [0; RING],
            n_input: 0,
            n_tick: 0,
            tick_hist: [0; TICK_HISTOGRAM_BUCKETS],
            empty_since: None,
            deltas_sent: 0,
            deltas_dropped_full: 0,
            inputs_dropped: 0,
            tick_overruns: 0,
            last_publish: Instant::now(),
            tick_hz: 128.0,
            publish_seq: 0,
        }
    }

    /// kernel→sim latency of one input, nanoseconds.
    pub fn note_input_latency(&mut self, arrival_ns: u64, now_ns: u64) {
        // saturating: a stale arrival stamp must never panic the sim thread.
        ring_push(&mut self.input_lat_ns, &mut self.n_input, now_ns.saturating_sub(arrival_ns));
    }

    /// wall duration of one `wasm.on_tick`.
    pub fn note_tick(&mut self, dur: Duration) {
        let us = dur.as_micros().min(u64::MAX as u128) as u64;
        ring_push(&mut self.tick_us, &mut self.n_tick, us);
        histogram_add(&mut self.tick_hist, us);
        if dur > tick_period(self.tick_hz) {
            self.tick_overruns += 1;
        }
    }

    pub fn snapshot(&self) -> StatusSnapshot {
        let lat = window(&self.input_lat_ns, self.n_input);
        let tick = window(&self.tick_us, self.n_tick);
        StatusSnapshot {
            tick_hz_est: self.tick_hz,
            tick_budget_p50_us: percentile(&tick, 50, 100),
            tick_budget_p99_us: percentile(&tick, 99, 100),
            tick_overruns: self.tick_overruns,
            kernel_to_sim_p50_us: percentile(&lat, 50, 100) / 1_000,
            kernel_to_sim_p99_us: percentile(&lat, 99, 100) / 1_000,
            tick_samples: tick.len() as u32,
            tick_histogram: self.tick_hist,
            players: 0,
            deltas_sent: self.deltas_sent,
            deltas_dropped_full: self.deltas_dropped_full,
            inputs_dropped: self.inputs_dropped,
            seq: self.publish_seq,
            updated_ms: mono_ns() / 1_000_000,
            // The session breakdown, nominal period and idle age are stamped by
            // `maybe_publish` from its [`PublishReport`], since only the loop
            // that owns the sockets knows what sits on each transport.
            ..Default::default()
        }
    }

    /// `true` when the next `maybe_publish` will actually roll a window, so a
    /// caller can skip building session rows in the middle of a tick.
    pub fn publish_due(&self) -> bool {
        self.last_publish.elapsed() >= Duration::from_secs(1)
    }

    /// Once per second: `snapshot()` plus the caller's session report →
    /// eprintln JSON on `stdout` (the DoD's latency report) and `try_send` to
    /// `bus.status_tx` for the Tokio plane. Returns `true` when a snapshot went
    /// out, so the caller knows whether its own window rolled over.
    pub fn maybe_publish(&mut self, bus: &Arc<Bus>, report: PublishReport) -> bool {
        if self.last_publish.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.tick_hz = if report.tick_hz_est.is_finite() && report.tick_hz_est > 0.0 {
            report.tick_hz_est
        } else {
            128.0
        };
        let mut snap = self.snapshot();
        snap.players = report.players;
        snap.nominal_tick_us = nominal_tick_us(report.tick_rate_hz);
        snap.ws_sessions = report.ws_sessions;
        snap.webrtc_sessions = report.webrtc_sessions;
        snap.udp_sessions = report.udp_sessions;
        snap.empty_for_sec = if report.players > 0
            || report.ws_sessions > 0
            || report.webrtc_sessions > 0
            || report.udp_sessions > 0
        {
            self.empty_since = None;
            0
        } else {
            // Armed on the first empty window, so the count starts the second
            // the room lost its last session rather than at process start.
            let since = *self.empty_since.get_or_insert(Instant::now());
            since.elapsed().as_secs()
        };
        let cap = if report.session_cap == 0 {
            MAX_CLIENT_ROWS
        } else {
            report.session_cap.min(MAX_CLIENT_ROWS)
        };
        let mut sessions = report.sessions;
        sessions.truncate(cap);
        snap.sessions = sessions;
        self.publish_seq = self.publish_seq.saturating_add(1);
        snap.seq = self.publish_seq;
        eprintln!("[status] {}", serde_json::json!(&snap));

        // Send-or-drop. `bus.status_rx` belongs to `status::pump`, which claims it
        // exactly once; evicting from it here would make the producer a second
        // consumer of its own queue and let it steal the pump's newest sample.
        // A full queue therefore means the consumer has stopped, not that it is
        // behind — and `/status` says so by ageing the sample it still holds.
        let _ = bus.status_tx.try_send(snap.clone());
        if let Ok(mut slot) = LAST.get_or_init(Mutex::default).lock() {
            *slot = Some(snap);
        }

        self.n_input = 0;
        self.n_tick = 0;
        self.tick_hist = [0; TICK_HISTOGRAM_BUCKETS];
        self.last_publish = Instant::now();
        true
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder::new()
    }
}

static LAST: OnceLock<Mutex<Option<StatusSnapshot>>> = OnceLock::new();

/// Last snapshot published (for any main-thread reader; lock-free read).
pub fn current() -> Option<StatusSnapshot> {
    LAST.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_config::{ControlConfig, GameConfig};
    use crate::rate_limit::IpRateLimiter;
    use crate::seats::SeatRegistry;
    use crate::signaling::SignalHub;
    use crate::static_routes::StaticServer;
    use crate::OnceReceiver;
    use axum::extract::State;
    use std::path::Path;

    /// A whole plane, built by hand: every field of `ControlState` is public
    /// precisely so telemetry can be tested without binding a port.
    fn state() -> ControlState {
        let game = GameConfig { name: "tank-3d".into(), ..GameConfig::default() };
        let control = ControlConfig { api_port: 0, ..ControlConfig::default() };
        let bus = Bus::new(16, 16);
        ControlState {
            statics: Arc::new(StaticServer::new(Path::new("."), &game)),
            game: Arc::new(game),
            control: Arc::new(control),
            signals: SignalHub::new("[]".into()),
            seats: Arc::new(std::sync::Mutex::new(SeatRegistry::new("test-secret"))),
            limiter: Arc::new(std::sync::Mutex::new(IpRateLimiter::new(20, 40))),
            port: 8124,
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            bus,
            started: Instant::now(),
        }
    }

    async fn json_of(response: Response) -> serde_json::Value {
        let (_, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, 4 << 20).await.expect("/status body");
        serde_json::from_slice(&bytes).expect("/status is JSON")
    }

    /// Every key a dashboard reads must be present even when nothing has
    /// happened yet — a missing key is a `undefined` in the browser, which is
    /// indistinguishable from a server that crashed.
    #[tokio::test]
    async fn status_reports_every_documented_group_on_an_idle_plane() {
        let body = json_of(status(State(state())).await).await;

        for path in [
            "/plane/uptime_ms",
            "/plane/controlPort",
            "/plane/workers",
            "/plane/game/name",
            "/plane/game/port",
            "/plane/game/webPort",
            "/seats/outstanding",
            "/seats/minted",
            "/seats/rejected",
            "/seats/refused",
            "/seats/bound",
            "/seats/ttlSecs",
            "/rateLimit/allowed",
            "/rateLimit/denied",
            "/rateLimit/squeezed",
            "/rateLimit/swept",
            "/rateLimit/tracked",
            "/rateLimit/rps",
            "/rateLimit/burst",
            "/signaling/peers",
            "/signaling/delivered",
            "/signaling/droppedFull",
            "/agones/enabled",
            "/agones/state",
            "/agones/ready",
            "/agones/intervalMs",
            "/agones/uptimeMs",
            // `PingStats` is `#[serde(flatten)]`, so its counters sit directly
            // on the `agones` object rather than under a nested group.
            "/agones/successes",
            "/agones/consecutiveFailures",
            "/bus/inputsQueued",
            "/bus/commandsQueued",
            "/bus/deltasQueued",
        ] {
            assert!(body.pointer(path).is_some(), "missing {path} in {body}");
        }
        assert_eq!(body["plane"]["controlPort"].as_u64(), Some(8124));
    }

    /// A process that never configured Agones must say so instead of looking
    /// broken: the group is present, and `enabled` is the only true fact about
    /// it. `state` is `Unknown`, not a plausible `Ready`.
    #[tokio::test]
    async fn agones_reports_absent_not_failing_when_no_sidecar_is_configured() {
        let body = json_of(status(State(state())).await).await;
        assert_eq!(body["agones"]["enabled"].as_bool(), Some(false));
        assert_eq!(body["agones"]["ready"].as_bool(), Some(false));
        assert_eq!(body["agones"]["pings"].as_u64(), Some(0), "no heartbeats sent");
    }

    /// A plane with no sim thread must say so. `updated_ms` is monotonic
    /// process time, so a zeroed snapshot would look like a room that ticked
    /// at startup and then went quiet forever.
    #[tokio::test]
    async fn a_missing_sim_sample_is_null_never_zeros() {
        let body = json_of(status(State(state())).await).await;
        assert!(body["sim"].is_null(), "sim must be null, got {}", body["sim"]);
    }

    /// Seats and rate-limit counters are the two numbers an operator watches
    /// when a room "won't let anyone in", so they must move with the real
    /// registries rather than being decorative.
    #[tokio::test]
    async fn counters_track_the_registries_they_report() {
        let s = state();
        s.seats.lock().unwrap().mint("tank-3d", 300);
        assert!(s.seats.lock().unwrap().verify("nonsense").is_none(), "forged token rejected");
        let ip = std::net::IpAddr::from([127, 0, 0, 1]);
        for _ in 0..41 {
            s.limiter.lock().unwrap().allow(ip);
        }
        // Held: `SeatReplies` detaches the route when it drops, so a temporary
        // here would leave `peers` back at zero before `/status` reads it.
        let _replies = s.signals.attach("seat-1");

        let body = json_of(status(State(s.clone())).await).await;
        assert_eq!(body["seats"]["minted"].as_u64(), Some(1));
        assert_eq!(body["seats"]["outstanding"].as_u64(), Some(1));
        assert_eq!(body["seats"]["rejected"].as_u64(), Some(1));
        assert_eq!(body["rateLimit"]["allowed"].as_u64(), Some(40), "burst is 40");
        assert_eq!(body["rateLimit"]["denied"].as_u64(), Some(1));
        assert_eq!(body["rateLimit"]["tracked"].as_u64(), Some(1));
        assert_eq!(body["rateLimit"]["rps"].as_u64(), Some(20));
        assert_eq!(body["signaling"]["peers"].as_u64(), Some(1));
    }

    /// `/api/rooms` describes the one room this process hosts, including the
    /// two ports a client needs: the game WS and this plane.
    #[tokio::test]
    async fn rooms_lists_the_hosted_room_with_its_ports() {
        // A real `Host` header, so the answer is an address the caller can dial
        // rather than the wildcard the server bound.
        let mut headers = axum::http::HeaderMap::new();
        headers
            .insert(axum::http::header::HOST, "arena.example:9999".parse().expect("valid host header"));
        let body = json_of(rooms(State(state()), headers).await).await;
        let room = &body[0];
        for key in [
            "name",
            "clients",
            "maxClients",
            "ready",
            "tickHz",
            "tickRate",
            "port",
            "controlPort",
            "serverId",
            "signalingUrl",
            "udpPort",
            "hasSimSample",
            "ageMs",
        ] {
            assert!(room.get(key).is_some(), "missing {key} in {room}");
        }
        assert_eq!(room["name"].as_str(), Some("tank-3d"));
        assert_eq!(room["controlPort"].as_u64(), Some(8124));
        // The caller's own host, re-pinned to the port the plane actually holds:
        // a `ws://<bind_addr>/ws/signal` would dial port 80 and look dead.
        assert_eq!(room["signalingUrl"].as_str(), Some("ws://arena.example:8124/ws/signal"));
        assert_eq!(room["serverId"].as_str(), Some("http://arena.example:8124"));
        assert_eq!(room["hasSimSample"].as_bool(), Some(false));
        assert!(room["ageMs"].is_null(), "no sample means no age");
    }

    /// A wildcard bind address is not a connectable host, and a missing `Host`
    /// must not turn the answer into one.
    #[test]
    fn a_wildcard_bind_never_leaks_into_the_dialable_host() {
        let mut headers = axum::http::HeaderMap::new();
        for wildcard in ["0.0.0.0", "::", "*"] {
            headers.insert(axum::http::header::HOST, wildcard.parse().expect("valid host header"));
            assert_eq!(client_host(&headers), "127.0.0.1", "bind_addr {wildcard}");
        }
        headers.insert(axum::http::header::HOST, "game.example:8124".parse().expect("valid host header"));
        assert_eq!(client_host(&headers), "game.example", "the advertised port wins over the header's");
    }

    /// `age_ms` is measured against the sim's own monotonic epoch, so a stale
    /// sample ages and a fresh one does not.
    #[tokio::test]
    async fn age_ms_uses_the_sims_monotonic_clock() {
        let stamp = mono_ns() / 1_000_000;
        tokio::time::sleep(Duration::from_millis(60)).await;
        let aged = StatusSnapshot { updated_ms: stamp, ..Default::default() };
        let age = age_ms(&aged);
        assert!((60..2_000).contains(&age), "a 60 ms-old stamp read as {age} ms");

        // A sample stamped in the future must read as zero age, never as a wrap.
        let ahead = StatusSnapshot { updated_ms: stamp + 1_000_000, ..Default::default() };
        assert_eq!(age_ms(&ahead), 0);
    }

    /// The pump is the only consumer of the status queue and keeps the newest
    /// of several queued samples.
    #[tokio::test]
    async fn pump_keeps_the_newest_snapshot_the_sim_publishes() {
        let s = state();
        let pump = tokio::spawn(pump(s.clone()));
        // Let the pump run to its first await: it takes the queue there, and
        // probing `try_take()` from here would steal it out from under it.
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }

        let now_ms = mono_ns() / 1_000_000;
        s.bus.status_tx.try_send(StatusSnapshot { seq: 1, ..Default::default() }).unwrap();
        s.bus.status_tx.try_send(StatusSnapshot {
            seq: 2,
            players: 3,
            tick_hz_est: 128.0,
            updated_ms: now_ms,
            ..Default::default()
        })
        .unwrap();

        for _ in 0..400 {
            if s.latest_status.lock().unwrap().as_ref().is_some_and(|x| x.seq == 2) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        pump.abort();

        let held = s.latest_status.lock().unwrap().clone().expect("pump stored the sample");
        assert_eq!(held.seq, 2, "the newest sample must win");
        assert_eq!(held.players, 3);

        // A second pump cannot double-consume the queue: `/status` degrades to
        // the last sample rather than erroring.
        assert!(s.status_rx.try_take().is_none(), "the pump owns the queue");

        let status_body = json_of(status(State(s.clone())).await).await;
        assert_eq!(status_body["sim"]["seq"].as_u64(), Some(2));
        assert_eq!(status_body["sim"]["players"].as_u64(), Some(3));
        assert!(status_body["sim"]["age_ms"].as_u64().unwrap() < 5_000);

        // `/api/rooms` reads the same sample, so occupancy and telemetry agree.
        let rooms_body = json_of(rooms(State(s), axum::http::HeaderMap::new()).await).await;
        assert_eq!(rooms_body[0]["clients"].as_u64(), Some(3));
        assert_eq!(rooms_body[0]["tickHz"].as_f64(), Some(128.0));
        assert_eq!(rooms_body[0]["hasSimSample"].as_bool(), Some(true));
        assert!(rooms_body[0]["ageMs"].as_u64().unwrap() < 5_000);
    }

    // ---- recorder tests (moved from src/threads/status.rs) ------------------

    #[test]
    fn percentiles_on_synthetic_samples() {
        let mut r = Recorder::new();
        for i in 0..100u64 {
            r.note_input_latency(0, (i + 1) * 1_000); // 1..=100 µs latencies
            r.note_tick(Duration::from_micros(i + 1));
        }
        let s = r.snapshot();
        assert_eq!(s.kernel_to_sim_p50_us, 50);
        assert_eq!(s.kernel_to_sim_p99_us, 99);
        assert_eq!(s.tick_budget_p50_us, 50);
        assert_eq!(s.tick_budget_p99_us, 99);
    }

    #[test]
    fn percentile_nearest_rank_small_window() {
        let mut r = Recorder::new();
        r.note_tick(Duration::from_micros(10));
        let s = r.snapshot();
        assert_eq!(s.tick_budget_p50_us, 10);
        assert_eq!(s.tick_budget_p99_us, 10);
        let empty = Recorder::new().snapshot();
        assert_eq!((empty.tick_budget_p50_us, empty.kernel_to_sim_p99_us), (0, 0));
    }

    #[test]
    fn ring_wraps_and_keeps_window() {
        let mut r = Recorder::new();
        // Overfill the ring: newest samples are all 1000 ns.
        for _ in 0..(RING as u64 * 2) {
            r.note_input_latency(0, 1_000);
        }
        let s = r.snapshot();
        assert_eq!(s.kernel_to_sim_p50_us, 1); // ns → µs
        assert_eq!(s.kernel_to_sim_p99_us, 1);
    }

    #[test]
    fn stale_arrival_saturates() {
        let mut r = Recorder::new();
        r.note_input_latency(10_000, 3); // clock went backwards: no panic, no underflow
        assert_eq!(r.snapshot().kernel_to_sim_p50_us, 0);
    }

    #[test]
    fn overrun_counted_against_tick_period() {
        let mut r = Recorder::new(); // 128 Hz → budget 7.8125 ms
        r.note_tick(Duration::from_millis(20));
        r.note_tick(Duration::from_millis(1));
        assert_eq!(r.snapshot().tick_overruns, 1);
    }

    fn publish_report() -> PublishReport {
        PublishReport { tick_hz_est: 128.0, tick_rate_hz: 128, players: 3, session_cap: 8, ..Default::default() }
    }

    #[test]
    fn publish_flows_to_bus_and_current() {
        let bus = Bus::new(16, 16);
        let mut r = Recorder::new();
        r.note_tick(Duration::from_micros(500));
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(&bus, publish_report()));
        let on_bus = bus.status_rx.try_recv().unwrap();
        assert_eq!(on_bus.players, 3);
        assert_eq!(on_bus.tick_budget_p50_us, 500);
        assert_eq!(current().unwrap().seq, on_bus.seq);
        // Window rings reset after publishing.
        assert_eq!(r.n_input, 0);
        assert_eq!(r.n_tick, 0);
        // Sub-second calls are no-ops, and say so.
        assert!(!r.publish_due());
        assert!(!r.maybe_publish(&bus, publish_report()));
        assert!(bus.status_rx.try_recv().is_err());
    }

    /// The histogram is the bar chart's only source, so it must count every
    /// tick of the window it describes and roll over with the rings.
    #[test]
    fn tick_histogram_accumulates_and_rolls_with_the_window() {
        let bus = Bus::new(16, 16);
        let mut r = Recorder::new();
        // 1 µs and 3 µs land in buckets 0 and 1; 5 ms is past the top bucket.
        r.note_tick(Duration::from_micros(1));
        r.note_tick(Duration::from_micros(3));
        r.note_tick(Duration::from_millis(5));
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(&bus, publish_report()));
        let on_bus = bus.status_rx.try_recv().unwrap();
        assert_eq!(on_bus.tick_samples, 3);
        assert_eq!(on_bus.tick_histogram[0], 1);
        assert_eq!(on_bus.tick_histogram[1], 1);
        assert_eq!(on_bus.tick_histogram[TICK_HISTOGRAM_BUCKETS - 1], 1);
        assert_eq!(on_bus.tick_histogram.iter().sum::<u32>(), 3);
        // The next window starts empty, even before anyone publishes again.
        let fresh = r.snapshot();
        assert_eq!(fresh.tick_histogram, [0; TICK_HISTOGRAM_BUCKETS]);
        assert_eq!(fresh.tick_samples, 0);
    }

    /// `nominal_tick_us` is the "are we keeping up" ruler, and `empty_for_sec`
    /// is the only proof an idle process is idle rather than unreachable.
    #[test]
    fn nominal_period_and_idle_age_come_from_the_report() {
        let bus = Bus::new(16, 16);
        let mut r = Recorder::new();
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(&bus, PublishReport { tick_rate_hz: 128, players: 1, ..Default::default() }));
        let on_bus = bus.status_rx.try_recv().unwrap();
        assert_eq!(on_bus.nominal_tick_us, 7_812);
        assert_eq!(on_bus.empty_for_sec, 0, "occupied rooms are never idle");

        r.last_publish = Instant::now() - Duration::from_secs(2);
        r.empty_since = Some(Instant::now() - Duration::from_secs(7));
        assert!(r.maybe_publish(&bus, PublishReport { tick_rate_hz: 64, ..Default::default() }));
        let idle = bus.status_rx.try_recv().unwrap();
        assert_eq!(idle.nominal_tick_us, 15_625);
        assert_eq!(idle.empty_for_sec, 7);

        // A loop that does not know its configured rate reports 0 rather than
        // the 128 Hz period it happens to default its overrun check to.
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(&bus, PublishReport::default()));
        assert_eq!(bus.status_rx.try_recv().unwrap().nominal_tick_us, 0);
    }

    #[test]
    fn a_publish_carries_the_session_rows_and_the_transport_mix() {
        let bus = Bus::new(16, 16);
        let mut r = Recorder::new();
        r.last_publish = Instant::now() - Duration::from_secs(2);
        let row = |session: &str, transport: &str| SessionTelemetry {
            session: session.into(),
            transport: transport.into(),
            player_id: Some(1),
            acked_seq: Some(42),
            idle_ms: Some(5),
        };
        assert!(r.maybe_publish(
            &bus,
            PublishReport {
                players: 2,
                ws_sessions: 1,
                udp_sessions: 1,
                sessions: vec![row("ws-1", "websocket"), row("[::1]:7778", "native-udp")],
                ..Default::default()
            }
        ));
        let on_bus = bus.status_rx.try_recv().unwrap();
        assert_eq!(on_bus.sessions.len(), 2);
        assert_eq!(on_bus.sessions[0].session, "ws-1");
        assert_eq!(on_bus.sessions[1].transport, "native-udp");
        assert_eq!(on_bus.sessions[0].acked_seq, Some(42));
        assert_eq!(on_bus.webrtc_sessions, 0);

        // The cap is a size guard on the payload, not a suggestion.
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(
            &bus,
            PublishReport { sessions: vec![row("a", "websocket"), row("b", "websocket")], session_cap: 1, ..Default::default() }
        ));
        assert_eq!(bus.status_rx.try_recv().unwrap().sessions.len(), 1);
    }

    /// A room that never ticked must publish a zeroed window rather than
    /// dividing by an empty ring.
    #[test]
    fn publishing_an_empty_window_is_safe() {
        let bus = Bus::new(16, 16);
        let mut r = Recorder::new();
        r.last_publish = Instant::now() - Duration::from_secs(2);
        assert!(r.maybe_publish(&bus, PublishReport::default()));
        let on_bus = bus.status_rx.try_recv().unwrap();
        assert_eq!(on_bus.tick_samples, 0);
        assert_eq!(on_bus.tick_histogram, [0; TICK_HISTOGRAM_BUCKETS]);
        assert_eq!((on_bus.tick_budget_p50_us, on_bus.tick_budget_p99_us), (0, 0));
        assert!(on_bus.sessions.is_empty());
    }
}
