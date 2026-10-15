use engine_dashboard::{Overview, TICK_HISTOGRAM_BUCKETS};

use super::{latest_sample, sample_age_ms};
use crate::ApiState;

pub fn build_overview(state: &ApiState) -> Overview {
    let snap = latest_sample(state);
    let has_sample = snap.is_some();
    let sample_age_ms = snap.as_ref().map(sample_age_ms).unwrap_or(0);
    let ccu = snap.as_ref().map(|s| s.players).unwrap_or(0);
    let (signals_delivered, signals_dropped) = state.signals.counts();

    let (seats_outstanding, seats_minted, seats_rejected, seats_refused, seats_bound) = match state.seats.lock() {
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
        capacity: engine_dashboard::Capacity { players: ccu, max: state.game.max_clients.max(0) as u32 },
        tick: engine_dashboard::TickHealth {
            hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
            configured_hz: state.game.tick_rate,
            nominal_us: snap.as_ref().map(|s| s.nominal_tick_us).unwrap_or(0),
            p50_us: snap.as_ref().map(|s| s.tick_budget_p50_us).unwrap_or(0),
            p99_us: snap.as_ref().map(|s| s.tick_budget_p99_us).unwrap_or(0),
            overruns: snap.as_ref().map(|s| s.tick_overruns).unwrap_or(0),
            samples: snap.as_ref().map(|s| s.tick_samples).unwrap_or(0),
            seq: snap.as_ref().map(|s| s.seq).unwrap_or(0),
            histogram: snap.as_ref().map(|s| s.tick_histogram).unwrap_or([0; TICK_HISTOGRAM_BUCKETS]),
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
        agones: serde_json::to_value(engine_agones::snapshot()).unwrap_or_else(|_| serde_json::json!({})),
        has_sim_sample: has_sample,
        sample_age_ms,
    }
}
