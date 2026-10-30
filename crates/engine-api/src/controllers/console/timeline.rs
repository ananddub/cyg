use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::timeline::TimelineQuery;
use crate::error::V2Error;
use crate::services::console::ConsoleService;

struct ConsoleTimelineController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/timeline")]
impl ConsoleTimelineController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/{accountId}")]
    async fn feed(
        &self,
        _auth: ConsoleAuthed,
        Path(account_id): Path<String>,
        Params(query): Params<TimelineQuery>,
    ) -> Response {
        let state = current_state();
        let Ok(account_id) = Uuid::parse_str(account_id.trim()) else {
            return V2Error::invalid_argument("That account id is not a UUID.").into_response();
        };
        self.service
            .player_timeline(&state, account_id, query)
            .await
            .into_response()
    }
}
