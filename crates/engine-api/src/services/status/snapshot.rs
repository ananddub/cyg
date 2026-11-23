use crate::services::plane::Plane;
use std::sync::Arc;

use axum::Json;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::StatusService;
use crate::utils::status::age_ms;

impl StatusService {
    pub async fn status(&self) -> Response {
        let snapshot = self
            .plane
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

        let (seats_minted, seats_rejected) = self
            .plane
            .seats
            .lock()
            .map(|r| r.counts())
            .unwrap_or((0, 0));
        let (seats_refused, seats_bound) = self
            .plane
            .seats
            .lock()
            .map(|r| (r.refused(), r.seat_bound()))
            .unwrap_or((0, 0));
        let (limit_allowed, limit_denied) = self.plane.limiter.counts();
        let (limit_squeezed, limit_swept) =
            (self.plane.limiter.squeezed(), self.plane.limiter.swept());
        let (signals_delivered, signals_dropped) = self.plane.signals.counts();

        Json(json!({
            "plane": {
                "uptime_ms": self.plane.started.elapsed().as_millis() as u64,
                "controlPort": self.plane.control_port(),
                "workers": self.plane.control.workers,
                "game": { "name": self.plane.game.name, "port": self.plane.game.port, "webPort": self.plane.game.web_port },
            },
            "sim": snapshot,
            "seats": {
                "outstanding": self.plane.seats.lock().map(|r| r.outstanding()).unwrap_or(0),
                "minted": seats_minted,
                "rejected": seats_rejected,
                "refused": seats_refused,
                "bound": seats_bound,
                "ttlSecs": self.plane.control.seat_ttl_secs,
            },
            "rateLimit": {
                "allowed": limit_allowed,
                "denied": limit_denied,
                "squeezed": limit_squeezed,
                "swept": limit_swept,
                "tracked": self.plane.limiter.tracked(),
                "rps": self.plane.control.rate_limit_rps,
                "burst": self.plane.control.rate_limit_burst,
            },
            "signaling": {
                "peers": self.plane.signals.peers(),
                "delivered": signals_delivered,
                "droppedFull": signals_dropped,
            },
            "agones": serde_json::to_value(engine_agones::snapshot())
                .unwrap_or_else(|_| json!({})),
            "bus": {
                "inputsQueued": self.plane.bus.inputs_rx.len(),
                "commandsQueued": self.plane.bus.commands_rx.len(),
                "deltasQueued": self.plane.bus.deltas_rx.len(),
            },
        }))
        .into_response()
    }
}

pub async fn status(plane: &Plane) -> Response {
    StatusService::new(Arc::new(plane.clone())).status().await
}
