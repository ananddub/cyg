use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::services::console::ConsoleService;

struct ConsoleRoomController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/room")]
impl ConsoleRoomController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    /// Who runs what, across the fleet. Read-only: the claim and the release are
    /// the hosts', and a console that could edit them would put an operator's
    /// guess above the node that is actually ticking the room.
    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_rooms(&state).await.into_response()
    }
}
