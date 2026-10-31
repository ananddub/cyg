use axum::Json;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiState;

use super::StatusService;
use crate::utils::status::age_ms;

impl StatusService {
    pub async fn status(&self, state: &ApiState) -> Response {
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

        let (seats_minted, seats_rejected) =
            state.seats.lock().map(|r| r.counts()).unwrap_or((0, 0));
        let (seats_refused, seats_bound) = state
            .seats
            .lock()
            .map(|r| (r.refused(), r.seat_bound()))
            .unwrap_or((0, 0));
        let (limit_allowed, limit_denied) =
            state.limiter.lock().map(|l| l.counts()).unwrap_or((0, 0));
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
            "agones": serde_json::to_value(engine_agones::snapshot())
                .unwrap_or_else(|_| json!({})),
            "bus": {
                "inputsQueued": state.bus.inputs_rx.len(),
                "commandsQueued": state.bus.commands_rx.len(),
                "deltasQueued": state.bus.deltas_rx.len(),
            },
        }))
        .into_response()
    }
}

pub async fn status(state: &ApiState) -> Response {
    StatusService.status(state).await
}
