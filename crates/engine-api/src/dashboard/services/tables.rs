use engine_dashboard::{RoomRow, WorkerRow};

use super::{latest_sample, sample_age_ms};
use crate::ApiState;

pub fn build_rooms(state: &ApiState) -> Vec<RoomRow> {
    let snap = latest_sample(state);
    let clients = snap.as_ref().map(|s| s.players).unwrap_or(0);
    let status = if clients > 0 { "live" } else { "empty" }.to_string();
    let host = "127.0.0.1";

    vec![RoomRow {
        name: state.game.name.clone(),
        clients,
        max_clients: state.game.max_clients.max(0) as u32,
        tick_hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
        tick_rate: state.game.tick_rate,
        port: state.game.port,
        control_port: state.control_port(),
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

pub fn build_workers(state: &ApiState) -> Vec<WorkerRow> {
    let snap = latest_sample(state);
    let mut rows = Vec::with_capacity(2);

    // No transport row: per-plane attribution isn't reachable from ApiState.
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
