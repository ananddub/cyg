use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::Params;
use crate::dto::matches::ListMatchesParams;
use crate::services::match_query::MatchQueryService;

struct MatchController {
    service: Arc<MatchQueryService>,
}

#[controller("/v2")]
impl MatchController {
    fn new(service: Arc<MatchQueryService>) -> Self {
        Self { service }
    }

    #[get("/match")]
    async fn list_matches(
        &self,
        _auth: Authed,
        Params(req): Params<ListMatchesParams>,
    ) -> Response {
        self.service.list_matches(req).into_response()
    }

    #[get("/matchmaker/stats")]
    async fn matchmaker_stats(&self, _auth: Authed) -> Response {
        self.service.matchmaker_stats().into_response()
    }
}
