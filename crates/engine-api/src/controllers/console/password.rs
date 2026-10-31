use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::user::ChangeConsolePasswordRequest;
use crate::services::console::ConsoleService;

struct ConsolePasswordController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/password_change")]
impl ConsolePasswordController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn change(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<ChangeConsolePasswordRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .change_console_password(&state, &auth, body)
            .await
            .into_response()
    }
}
