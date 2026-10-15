//! Auto-registered binding for `GET /v2/console/user` — the console operators.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

struct ConsoleUserController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/user")]
impl ConsoleUserController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_console_users(&state, page)
            .await
            .into_response()
    }
}
