use std::sync::Arc;

use auto_route::controller;
use axum::http::HeaderMap;
use axum::response::Response;

use crate::current_state;
use crate::services::status::StatusService;

/// The lobby is a directory, not a second source of truth: it lists the rooms
/// the host is actually running, with the same occupancy and phase the status
/// endpoint reports. A client subscribes to room patches through its lobby
/// room; this route is the cold-start snapshot.
struct LobbyController {
    service: Arc<StatusService>,
}

#[controller("/lobby")]
impl LobbyController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("/rooms")]
    async fn rooms(&self, headers: HeaderMap) -> Response {
        let state = current_state();
        self.service.rooms(&state, headers).await
    }
}

struct ApiLobbyController {
    service: Arc<StatusService>,
}

#[controller("/api/lobby")]
impl ApiLobbyController {
    fn new(service: Arc<StatusService>) -> Self {
        Self { service }
    }

    #[get("/rooms")]
    async fn rooms(&self, headers: HeaderMap) -> Response {
        let state = current_state();
        self.service.rooms(&state, headers).await
    }
}
