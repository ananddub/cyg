use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::services::console::ConsoleService;

struct ConsoleStatusController {
    service: Arc<ConsoleService>,
}

type RouteRowList = Vec<crate::dto::console::status::RouteRow>;
#[controller("/v1/console")]
impl ConsoleStatusController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/status", response_body = crate::dto::console::status::StatusResponse)]
    async fn status(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.status().await.into_response()
    }

    #[get("/route", response_body = RouteRowList)]
    async fn routes(&self, _auth: ConsoleAuthed) -> Response {
        self.service.routes().await.into_response()
    }
}
