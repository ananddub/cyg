use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::{Authed, ConsoleAuthed};
use crate::app::params::Params;
use crate::current_state;
use crate::dto::match_history::{DesyncParams, HistoryParams};
use crate::services::match_history::MatchHistoryService;

struct MatchHistoryController {
    service: Arc<MatchHistoryService>,
}

#[controller("/v2")]
impl MatchHistoryController {
    fn new(service: Arc<MatchHistoryService>) -> Self {
        Self { service }
    }

    #[get("/match/{id}/events")]
    async fn events(
        &self,
        _auth: Authed,
        Path(id): Path<String>,
        Params(req): Params<HistoryParams>,
    ) -> Response {
        let state = current_state();
        self.service
            .events(&state, &id, req.limit)
            .await
            .into_response()
    }

    #[get("/match/{id}/history")]
    async fn history(
        &self,
        _auth: Authed,
        Path(id): Path<String>,
        Params(req): Params<HistoryParams>,
    ) -> Response {
        let state = current_state();
        self.service
            .history(&state, &id, req.limit)
            .await
            .into_response()
    }
}

struct DesyncController {
    service: Arc<MatchHistoryService>,
}

#[controller("/v2/console")]
impl DesyncController {
    fn new(service: Arc<MatchHistoryService>) -> Self {
        Self { service }
    }

    #[get("/desync")]
    async fn desyncs(&self, _auth: ConsoleAuthed, Params(req): Params<DesyncParams>) -> Response {
        let state = current_state();
        self.service
            .desyncs(&state, req.room.as_deref(), req.limit)
            .await
            .into_response()
    }

    #[get("/match/{id}/replay")]
    async fn replay(&self, _auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.replay(&state, &id).await.into_response()
    }
}
