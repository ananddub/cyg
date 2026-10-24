//! Auto-registered bindings for the player match REST surface: `GET /v2/match` and
//! `GET /v2/matchmaker/stats`.
//!
//! Both are session-gated like the rest of `/v2`, and both read the in-process
//! registries through the match query service; the controller only decodes and
//! prints.

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

    /// List active realtime matches.
    #[get("/match")]
    async fn list_matches(
        &self,
        _auth: Authed,
        Params(req): Params<ListMatchesParams>,
    ) -> Response {
        self.service.list_matches(req).into_response()
    }

    /// Matchmaker queue statistics.
    #[get("/matchmaker/stats")]
    async fn matchmaker_stats(&self, _auth: Authed) -> Response {
        self.service.matchmaker_stats().into_response()
    }
}
