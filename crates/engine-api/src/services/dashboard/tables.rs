use std::sync::Arc;

use crate::services::plane::Plane;

use engine_dashboard::{RoomRow, WorkerRow};
use uuid::Uuid;

use super::{latest_sample, sample_age_ms};

pub async fn build_rooms_scoped(plane: &Plane, target_game_id: Option<Uuid>) -> Vec<RoomRow> {
    let snap = latest_sample(plane);
    let clients = snap.as_ref().map(|s| s.players).unwrap_or(0);
    let host = "127.0.0.1";

    if let Ok(game_list) = crate::services::games::GamesService::new(Arc::new(plane.clone()))
        .list_games()
        .await
    {
        let mut rooms = Vec::new();
        for game in game_list.games {
            if let Some(target) = target_game_id {
                if game.id != target {
                    continue;
                }
            }

            let is_current = plane.game.name == game.name;
            let game_clients = if is_current { clients } else { 0 };
            let game_status = if game_clients > 0 { "live" } else { "empty" }.to_string();
            let max_clients =
                game.config
                    .get("max_clients")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(plane.game.max_clients.max(0) as u64) as u32;
            let tick_rate = game
                .config
                .get("tick_rate")
                .and_then(|v| v.as_u64())
                .unwrap_or(plane.game.tick_rate as u64) as u32;

            rooms.push(RoomRow {
                name: game.name.clone(),
                register_id: Some(game.game_key.clone()),
                game_id: Some(game.id.to_string()),
                game_name: Some(game.name.clone()),
                clients: game_clients,
                max_clients,
                tick_hz: if is_current {
                    snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0)
                } else {
                    tick_rate as f64
                },
                tick_rate,
                port: plane.game.port,
                control_port: plane.control_port(),
                empty_for_sec: match &snap {
                    Some(s) if game_clients == 0 => Some(s.empty_for_sec),
                    _ => None,
                },
                status: game_status,
                has_sim_sample: is_current && snap.is_some(),
                sample_age_ms: if is_current {
                    snap.as_ref().map(sample_age_ms)
                } else {
                    None
                },
                signaling_url: format!("ws://{host}:{}/ws/signal", plane.control_port()),
                server_id: format!("http://{host}:{}", plane.control_port()),
            });
        }
        if !rooms.is_empty() || target_game_id.is_some() {
            return rooms;
        }
    }

    vec![RoomRow {
        name: plane.game.name.clone(),
        register_id: Some(plane.game.name.clone()),
        game_id: None,
        game_name: Some(plane.game.name.clone()),
        clients,
        max_clients: plane.game.max_clients.max(0) as u32,
        tick_hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
        tick_rate: plane.game.tick_rate,
        port: plane.game.port,
        control_port: plane.control_port(),
        empty_for_sec: match &snap {
            Some(s) if clients == 0 => Some(s.empty_for_sec),
            _ => None,
        },
        status: if clients > 0 { "live" } else { "empty" }.to_string(),
        has_sim_sample: snap.is_some(),
        sample_age_ms: snap.as_ref().map(sample_age_ms),
        signaling_url: format!("ws://{host}:{}/ws/signal", plane.control_port()),
        server_id: format!("http://{host}:{}", plane.control_port()),
    }]
}

pub fn build_rooms(plane: &Plane) -> Vec<RoomRow> {
    let snap = latest_sample(plane);
    let clients = snap.as_ref().map(|s| s.players).unwrap_or(0);
    let status = if clients > 0 { "live" } else { "empty" }.to_string();
    let host = "127.0.0.1";

    vec![RoomRow {
        name: plane.game.name.clone(),
        register_id: Some(plane.game.name.clone()),
        game_id: None,
        game_name: Some(plane.game.name.clone()),
        clients,
        max_clients: plane.game.max_clients.max(0) as u32,
        tick_hz: snap.as_ref().map(|s| s.tick_hz_est).unwrap_or(0.0),
        tick_rate: plane.game.tick_rate,
        port: plane.game.port,
        control_port: plane.control_port(),
        empty_for_sec: match &snap {
            Some(s) if clients == 0 => Some(s.empty_for_sec),
            _ => None,
        },
        status,
        has_sim_sample: snap.is_some(),
        sample_age_ms: snap.as_ref().map(sample_age_ms),
        signaling_url: format!("ws://{host}:{}/ws/signal", plane.control_port()),
        server_id: format!("http://{host}:{}", plane.control_port()),
    }]
}

pub fn build_workers(plane: &Plane) -> Vec<WorkerRow> {
    let snap = latest_sample(plane);
    let mut rows = Vec::with_capacity(2);

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
        threads: plane.control.workers as u32,
        queue_depth: Some(plane.bus.inputs_rx.len() + plane.bus.commands_rx.len()),
        live: true,
        ..Default::default()
    });

    rows
}
