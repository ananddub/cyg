use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::economy::EconomySummaryQuery;
use crate::services::console::ConsoleService;

struct ConsoleEconomyController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/economy")]
impl ConsoleEconomyController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/summary")]
    async fn summary(
        &self,
        _auth: ConsoleAuthed,
        Params(query): Params<EconomySummaryQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .economy_summary(&state, query)
            .await
            .into_response()
    }
}
