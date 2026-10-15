//! Dashboard REST routes: `/api/dashboard/{overview,rooms,workers,clients,ticks}`.
//!
//! Owned by the dashboard workstream. `lib.rs` merges [`router`] into the plane;
//! nothing else may add a `/api/dashboard/*` route.
//!
//! Every number here is already paid for somewhere: the sim's
//! [`StatusSnapshot`], the bus' own queue depths, the seat registry, the rate
//! limiter, the signaling hub and the Agones snapshot. The wire shapes live in
//! `engine_dashboard`, so these routes, `/ws/dashboard`, the Rust tests and the
//! frontend's `types.ts` all describe the same object.
//!
//! A field with no source stays at its default rather than being guessed: this
//! server has never measured a round trip, so nothing here invents one.

use std::sync::{Arc, OnceLock};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use engine_dashboard::{ClientRow, MAX_CLIENT_ROWS, Overview, RoomRow, StatusSnapshot, TickPoint, TickSeries, WorkerRow};

use crate::ControlState;

/// The ring the live graphs read. `status::pump` appends to it for every sample
/// it takes off the bus.
///
/// A process global rather than a `ControlState` field on purpose: the state is
/// built by hand in four test helpers, and a dashboard ring is not something a
/// route should have to thread through every one of them. Same shape as
/// [`crate::status::current`].
static HISTORY: OnceLock<Arc<engine_dashboard::DashboardHistory>> = OnceLock::new();

pub fn history() -> Arc<engine_dashboard::DashboardHistory> {
    HISTORY.get_or_init(Default::default).clone()
}

/// Called by `status::pump` for every sample it stores.
///
/// Stamped with the process monotonic clock, not the ring's own epoch:
/// `TickPoint::at_ms` and `Overview::sample_age_ms` must be the same clock or a
/// chart's right edge and the age readout disagree.
pub fn record_sample(snapshot: StatusSnapshot) {
    history().push(snapshot, crate::status::mono_ns() / 1_000_000);
}

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new()
        .route("/api/dashboard/overview", get(overview))
        .route("/api/dashboard/rooms", get(rooms))
        .route("/api/dashboard/workers", get(workers))
        .route("/api/dashboard/clients", get(clients))
        .route("/api/dashboard/ticks", get(ticks))
}

// The `build_*` functions are the seam: the routes below are thin, and
// `/ws/dashboard` calls exactly these, so a poll and a push can never disagree.

/// The newest sample the pump has stored, if the sim has ever published.
fn latest_sample(state: &ControlState) -> Option<StatusSnapshot> {
    state.latest_status.lock().ok().and_then(|g| g.clone())
}

/// Age of a sample against the sim's own monotonic epoch, like `status::age_ms`.
fn sample_age_ms(snap: &StatusSnapshot) -> u64 {
    (crate::status::mono_ns() / 1_000_000).saturating_sub(snap.updated_ms)
}

pub fn build_overview(state: &ControlState) -> Overview {
    let snap = latest_sample(state);
    let has_sample = snap.is_some();
    let sample_age_ms = snap.as_ref().map(sample_age_ms).unwrap_or(0);
    let ccu = snap.as_ref().map(|s| s.players).unwrap_or(0);
    let (signals_delivered, signals_dropped) = state.signals.counts();

    let (seats_outstanding, seats_minted, seats_rejected, seats_refused, seats_bound) = match state
        .seats
        .lock()
    {
        Ok(r) => (r.outstanding(), r.counts().0, r.counts().1, r.refused(), r.seat_bound()),
        Err(_) => (0, 0, 0, 0, 0),
    };
    let (limit_allowed, limit_denied, limit_squeezed, limit_swept, limit_tracked) = match state.limiter.lock() {
        Ok(l) => (l.counts().0, l.counts().1, l.squeezed(), l.swept(), l.tracked()),
        Err(_) => (0, 0, 0, 0, 0),
    };

    Overview {
        server: engine_dashboard::ServerInfo {
            name: state.game.name.clone(),
            port: state.game.port,
            control_port: state.control_port(),
            web_port: state.game.web_port,
        },
        uptime_sec: state.started.elapsed().as_secs(),
        ccu,
        rooms: 1,
        capacity: engine_dashboard::Capacity {
            players: ccu,
            max: state.game.max_clients.max(0) as u32,
        },
        tick: engine_dashboard::TickHealth {
            hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
            configured_hz: state.game.tick_rate,
            nominal_us: snap.as_ref().map(|s| s.nominal_tick_us).unwrap_or(0),
            p50_us: snap.as_ref().map(|s| s.tick_budget_p50_us).unwrap_or(0),
            p99_us: snap.as_ref().map(|s| s.tick_budget_p99_us).unwrap_or(0),
            overruns: snap.as_ref().map(|s| s.tick_overruns).unwrap_or(0),
            samples: snap.as_ref().map(|s| s.tick_samples).unwrap_or(0),
            seq: snap.as_ref().map(|s| s.seq).unwrap_or(0),
            histogram: snap.as_ref().map(|s| s.tick_histogram).unwrap_or([0; engine_dashboard::TICK_HISTOGRAM_BUCKETS]),
        },
        transport: engine_dashboard::TransportMix {
            websocket: snap.as_ref().map(|s| s.ws_sessions).unwrap_or(0),
            webrtc: snap.as_ref().map(|s| s.webrtc_sessions).unwrap_or(0),
            native_udp: snap.as_ref().map(|s| s.udp_sessions).unwrap_or(0),
            signal_peers: state.signals.peers() as u32,
            signals_delivered,
            signals_dropped,
        },
        bus: engine_dashboard::BusHealth {
            inputs_queued: state.bus.inputs_rx.len(),
            commands_queued: state.bus.commands_rx.len(),
            deltas_queued: state.bus.deltas_rx.len(),
            deltas_sent: snap.as_ref().map(|s| s.deltas_sent).unwrap_or(0),
            deltas_dropped_full: snap.as_ref().map(|s| s.deltas_dropped_full).unwrap_or(0),
            inputs_dropped: snap.as_ref().map(|s| s.inputs_dropped).unwrap_or(0),
        },
        seats: engine_dashboard::SeatHealth {
            outstanding: seats_outstanding,
            minted: seats_minted,
            rejected: seats_rejected,
            refused: seats_refused,
            bound: seats_bound,
            ttl_secs: state.control.seat_ttl_secs,
        },
        rate_limit: engine_dashboard::RateLimitHealth {
            allowed: limit_allowed,
            denied: limit_denied,
            squeezed: limit_squeezed,
            swept: limit_swept,
            tracked: limit_tracked,
            rps: state.control.rate_limit_rps,
            burst: state.control.rate_limit_burst,
        },
        agones: serde_json::to_value(crate::agones::status_snapshot())
            .unwrap_or_else(|_| serde_json::json!({})),
        has_sim_sample: has_sample,
        sample_age_ms,
    }
}

pub fn build_rooms(state: &ControlState) -> Vec<RoomRow> {
    let snap = latest_sample(state);
    let clients = snap.as_ref().map(|s| s.players).unwrap_or(0);
    // One room per process in this topology, so `live`/`empty` is pure
    // arithmetic on `clients` and can never disagree with it.
    let status = if clients > 0 { "live" } else { "empty" }.to_string();
    // The REST seam has no request headers, so this is `status::client_host`'s
    // no-Host fallback. The dashboard itself is same-origin and uses relative
    // URLs; these fields are for external readers pointed at this host.
    let host = "127.0.0.1";

    vec![RoomRow {
        name: state.game.name.clone(),
        clients,
        max_clients: state.game.max_clients.max(0) as u32,
        tick_hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
        tick_rate: state.game.tick_rate,
        port: state.game.port,
        control_port: state.control_port(),
        // `None` while players are in, unknown (`None`) until the sim has ever
        // published: an absent sample has no age to report, let alone an age.
        empty_for_sec: match &snap {
            Some(s) if clients == 0 => Some(s.empty_for_sec),
            _ => None,
        },
        status,
        has_sim_sample: snap.is_some(),
        sample_age_ms: snap.as_ref().map(sample_age_ms),
        signaling_url: format!("ws://{host}:{}/ws/signal", state.control_port()),
        server_id: format!("http://{host}:{}", state.control_port()),
    }]
}

pub fn build_workers(state: &ControlState) -> Vec<WorkerRow> {
    let snap = latest_sample(state);
    let mut rows = Vec::with_capacity(2);

    // The game loop is one pinned thread; a transport row would need thread
    // and per-plane delta attribution that ControlState cannot reach, so the
    // honest answer is not to draw one.
    rows.push(WorkerRow {
        name: "sim".into(),
        kind: "game".into(),
        threads: 1,
        ccu: snap.as_ref().map(|s| s.players).unwrap_or(0),
        tick_p50_us: snap.as_ref().map(|s| s.tick_budget_p50_us),
        tick_p99_us: snap.as_ref().map(|s| s.tick_budget_p99_us),
        nominal_tick_us: snap.as_ref().map(|s| s.nominal_tick_us),
        overruns: snap.as_ref().map(|s| s.tick_overruns),
        deltas_sent: snap.as_ref().map(|s| s.deltas_sent),
        deltas_dropped: snap.as_ref().map(|s| s.deltas_dropped_full),
        live: snap.is_some(),
        ..Default::default()
    });

    rows.push(WorkerRow {
        name: "control".into(),
        kind: "tokio".into(),
        threads: state.control.workers as u32,
        queue_depth: Some(state.bus.inputs_rx.len() + state.bus.commands_rx.len()),
        live: true,
        ..Default::default()
    });

    rows
}

pub fn build_clients(_state: &ControlState) -> Vec<ClientRow> {
    let room = _state.game.name.clone();
    let Some(entry) = history().newest() else {
        return Vec::new();
    };
    entry
        .snapshot
        .sessions
        .iter()
        .take(MAX_CLIENT_ROWS)
        .map(|s| ClientRow {
            session: s.session.clone(),
            transport: s.transport.clone(),
            room: room.clone(),
            player_id: s.player_id,
            acked_seq: s.acked_seq,
            idle_ms: s.idle_ms,
        })
        .collect()
}

pub fn build_ticks() -> TickSeries {
    let entries = history().snapshot_all();
    let mut prev_ms: Option<u64> = None;
    let points = entries
        .iter()
        .map(|e| {
            // The sim publishes once a second; more than 2.5 s between
            // arrivals means a sample never reached this plane, and the chart
            // must break rather than draw a flat run through the hole.
            let gap = prev_ms.is_some_and(|p| e.arrived_ms.saturating_sub(p) > 2_500);
            prev_ms = Some(e.arrived_ms);
            TickPoint {
                at_ms: e.arrived_ms,
                p50_us: e.snapshot.tick_budget_p50_us,
                p99_us: e.snapshot.tick_budget_p99_us,
                hz: e.snapshot.tick_hz_est,
                players: e.snapshot.players,
                seq: e.snapshot.seq,
                gap,
            }
        })
        .collect();
    let histogram = entries.last().map(|e| e.snapshot.tick_histogram).unwrap_or([0; engine_dashboard::TICK_HISTOGRAM_BUCKETS]);
    TickSeries { interval_sec: 1, points, histogram, bucket_edges_us: engine_dashboard::bucket_edges_us() }
}

async fn overview(State(state): State<ControlState>) -> Response {
    Json(build_overview(&state)).into_response()
}

async fn rooms(State(state): State<ControlState>) -> Response {
    Json(build_rooms(&state)).into_response()
}

async fn workers(State(state): State<ControlState>) -> Response {
    Json(build_workers(&state)).into_response()
}

async fn clients(State(state): State<ControlState>) -> Response {
    Json(build_clients(&state)).into_response()
}

async fn ticks(State(_state): State<ControlState>) -> Response {
    Json(build_ticks()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OnceReceiver;
    use crate::rate_limit::IpRateLimiter;
    use crate::seats::SeatRegistry;
    use crate::signaling::SignalHub;
    use crate::static_routes::StaticServer;
    use engine_bus::{Bus, SessionTelemetry};
    use engine_config::{ControlConfig, GameConfig};
    use std::path::Path;
    use std::time::Instant;

    /// Copy of `status::tests::state`: every field of `ControlState` is public
    /// so the seam can be tested without binding a port.
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

    /// HISTORY is a process global: keep this module's ring tests from racing
    /// each other. A sample from `status`'s pump test can still land inside a
    /// window, which the retry loops below absorb.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn an_idle_overview_serialises_every_key_as_zero_not_omission() {
        let s = state();
        let json = serde_json::to_value(build_overview(&s)).unwrap();
        for path in [
            "/server/name",
            "/server/port",
            "/server/controlPort",
            "/server/webPort",
            "/uptimeSec",
            "/ccu",
            "/rooms",
            "/capacity/players",
            "/capacity/max",
            "/tick/hz",
            "/tick/configuredHz",
            "/tick/nominalUs",
            "/tick/p50Us",
            "/tick/p99Us",
            "/tick/overruns",
            "/tick/samples",
            "/tick/seq",
            "/tick/histogram",
            "/transport/websocket",
            "/transport/webrtc",
            "/transport/nativeUdp",
            "/transport/signalPeers",
            "/transport/signalsDelivered",
            "/transport/signalsDropped",
            "/bus/inputsQueued",
            "/bus/commandsQueued",
            "/bus/deltasQueued",
            "/bus/deltasSent",
            "/bus/deltasDroppedFull",
            "/bus/inputsDropped",
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
            "/agones",
            "/hasSimSample",
            "/sampleAgeMs",
        ] {
            assert!(json.pointer(path).is_some(), "missing {path} in {json}");
        }
        // Identity is sourced even before the sim speaks.
        assert_eq!(json["server"]["name"].as_str(), Some("tank-3d"));
        assert_eq!(json["server"]["controlPort"].as_u64(), Some(8124));
        assert_eq!(json["ccu"].as_u64(), Some(0));
        assert_eq!(json["tick"]["hz"].as_f64(), Some(0.0));
        assert_eq!(json["tick"]["histogram"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).sum::<u64>(), 0);
        assert_eq!(json["hasSimSample"].as_bool(), Some(false));
        assert_eq!(json["sampleAgeMs"].as_u64(), Some(0));

        let rooms = build_rooms(&s);
        assert_eq!(rooms.len(), 1, "one room per process");
        assert_eq!(rooms[0].status, "empty");
        assert!(rooms[0].sample_age_ms.is_none(), "no sample means no age, not zero");
        assert_eq!(rooms[0].server_id, "http://127.0.0.1:8124");
        assert_eq!(rooms[0].signaling_url, "ws://127.0.0.1:8124/ws/signal");

        let workers = build_workers(&s);
        assert_eq!(workers[0].name, "sim");
        assert!(!workers[0].live, "the sim has never published");
        assert!(workers[0].tick_p50_us.is_none());
        assert_eq!(workers[1].name, "control");
        assert!(workers[1].live);
    }

    /// A sample in `latest_status` flips the derived state without touching
    /// anything stored: `live` is arithmetic on `clients`.
    #[test]
    fn a_fed_snapshot_makes_the_room_live_with_an_age() {
        let s = state();
        *s.latest_status.lock().unwrap() = Some(StatusSnapshot {
            seq: 7,
            players: 2,
            empty_for_sec: 41,
            updated_ms: crate::status::mono_ns() / 1_000_000,
            ..Default::default()
        });
        let rooms = build_rooms(&s);
        assert_eq!(rooms[0].status, "live");
        assert_eq!(rooms[0].clients, 2);
        assert_eq!(rooms[0].empty_for_sec, None, "a room with players is not empty");
        assert!(rooms[0].has_sim_sample);
        assert!(rooms[0].sample_age_ms.unwrap() < 5_000);
        assert_eq!(build_overview(&s).ccu, 2);
        assert!(build_workers(&s)[0].live);
    }

    #[test]
    fn a_skipped_sample_reads_as_a_gap_not_a_flat_line() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let h = history();
        // Base in the past so a concurrent real-time pump sample can only
        // append after ours, never between; retry absorbs one inside the push.
        let t0 = (crate::status::mono_ns() / 1_000_000).saturating_sub(20_000);
        let mut series = build_ticks();
        for _ in 0..3 {
            h.clear();
            h.push(StatusSnapshot { seq: 9101, tick_budget_p50_us: 100, ..Default::default() }, t0);
            h.push(StatusSnapshot { seq: 9102, tick_budget_p50_us: 200, ..Default::default() }, t0 + 1_000);
            h.push(StatusSnapshot {
                seq: 9103,
                tick_budget_p50_us: 300,
                tick_histogram: [1; engine_dashboard::TICK_HISTOGRAM_BUCKETS],
                ..Default::default()
            }, t0 + 4_000);
            series = build_ticks();
            if series.points.len() == h.len() && h.len() == 3 {
                break;
            }
        }
        assert_eq!(series.points.len(), 3, "one point per ring entry");
        assert_eq!(series.interval_sec, 1);
        assert_eq!(series.bucket_edges_us, engine_dashboard::bucket_edges_us());
        assert_eq!(series.histogram, [1; engine_dashboard::TICK_HISTOGRAM_BUCKETS], "newest window");
        assert_eq!(series.points.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![9101, 9102, 9103]);
        assert_eq!(series.points.iter().map(|p| p.gap).collect::<Vec<_>>(), vec![false, false, true]);
        assert_eq!(series.points[1].at_ms, t0 + 1_000);
        assert_eq!(series.points[1].p50_us, 200);
    }

    #[test]
    fn client_rows_map_the_newest_sessions_in_order_and_cap() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let h = history();
        let total = MAX_CLIENT_ROWS + 50;
        let sessions: Vec<SessionTelemetry> = (0..total)
            .map(|i| SessionTelemetry {
                session: format!("cap-{i}"),
                transport: "websocket".into(),
                player_id: Some(i as i64),
                acked_seq: Some(i as u32),
                idle_ms: Some(i as u64),
            })
            .collect();
        let s = state();
        let mut rows = Vec::new();
        for _ in 0..3 {
            h.clear();
            h.push(StatusSnapshot { seq: 9500, players: total as u32, sessions: sessions.clone(), ..Default::default() }, crate::status::mono_ns() / 1_000_000);
            rows = build_clients(&s);
            if rows.len() == MAX_CLIENT_ROWS {
                break;
            }
        }
        assert_eq!(rows.len(), MAX_CLIENT_ROWS, "a runaway registry cannot size the response");
        assert_eq!(rows[0].session, "cap-0");
        assert_eq!(rows.last().unwrap().session, format!("cap-{}", MAX_CLIENT_ROWS - 1), "in order");
        assert!(rows.iter().all(|r| r.room == "tank-3d" && r.transport == "websocket" && r.idle_ms.is_some()));
    }
}
