//! Auto-registered bindings for `/v2/tournament*`.
//!
//! Methods return `Response` (not `Json<T>`) so `auto_route` does not require
//! `T: poem_openapi::Type`; the wire types here are plain `serde` and the
//! response bytes are unchanged.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::tournament::{ListQuery, RecordsQuery};
use crate::services::tournament::TournamentService;

struct TournamentController {
    service: Arc<TournamentService>,
}

#[controller("/v2/tournament")]
impl TournamentController {
    fn new(service: Arc<TournamentService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list_tournaments(&self, auth: Authed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_tournaments(&state, &auth, req)
            .await
            .into_response()
    }

    #[get("/{tournament_id}")]
    async fn list_records(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Params(req): Params<RecordsQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records(&state, &auth, &id, req)
            .await
            .into_response()
    }

    #[post("/{tournament_id}")]
    async fn write_record(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Body(body): Body<Value>,
    ) -> Response {
        let state = current_state();
        self.service
            .write_record(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[put("/{tournament_id}")]
    async fn write_record_replace(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Body(body): Body<Value>,
    ) -> Response {
        let state = current_state();
        self.service
            .write_record(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{tournament_id}")]
    async fn delete_record(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_record(&state, &auth, &id)
            .await
            .into_response()
    }

    #[get("/{tournament_id}/owner/{owner_id}")]
    async fn list_records_around_owner(
        &self,
        auth: Authed,
        Path((id, owner_id)): Path<(String, String)>,
        Params(req): Params<RecordsQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records_around_owner(&state, &auth, &id, &owner_id, req)
            .await
            .into_response()
    }

    #[post("/{tournament_id}/join")]
    async fn join_tournament(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .join_tournament(&state, &auth, &id)
            .await
            .into_response()
    }
}
