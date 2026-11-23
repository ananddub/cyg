use crate::dto::leaderboard::RecordResponse;
use crate::dto::leaderboard::{RecordListResponse, RecordRequest};
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

#[controller("/v1/tournament")]
impl TournamentController {
    fn new(service: Arc<TournamentService>) -> Self {
        Self { service }
    }

    #[get("", response_body = crate::dto::tournament::TournamentListResponse)]
    async fn list_tournaments(&self, auth: Authed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.service
            .list_tournaments(&auth, req)
            .await
            .into_response()
    }

    #[get("/{tournament_id}", response_body = RecordListResponse)]
    async fn list_records(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Params(req): Params<RecordsQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records(&auth, &id, req)
            .await
            .into_response()
    }

    #[post("/{tournament_id}", request_body = RecordRequest, response_body = RecordResponse)]
    async fn write_record(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Body(body): Body<Value>,
    ) -> Response {
        let state = current_state();
        self.service
            .write_record(&auth, &id, body)
            .await
            .into_response()
    }

    #[put("/{tournament_id}", request_body = RecordRequest, response_body = RecordResponse)]
    async fn write_record_replace(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Body(body): Body<Value>,
    ) -> Response {
        let state = current_state();
        self.service
            .write_record(&auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{tournament_id}", response_body = crate::dto::common::Empty)]
    async fn delete_record(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service.delete_record(&auth, &id).await.into_response()
    }

    #[get("/{tournament_id}/owner/{owner_id}", response_body = RecordListResponse)]
    async fn list_records_around_owner(
        &self,
        auth: Authed,
        Path((id, owner_id)): Path<(String, String)>,
        Params(req): Params<RecordsQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records_around_owner(&auth, &id, &owner_id, req)
            .await
            .into_response()
    }

    #[post("/{tournament_id}/join", response_body = crate::dto::common::Empty)]
    async fn join_tournament(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .join_tournament(&auth, &id)
            .await
            .into_response()
    }
}
