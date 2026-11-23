use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::leaderboard::{HaystackQuery, ListQuery};
use crate::services::leaderboard::LeaderboardService;
use crate::services::leaderboard::define::DefineRequest;

struct LeaderboardController {
    service: Arc<LeaderboardService>,
}

#[controller("/v1/leaderboard")]
impl LeaderboardController {
    fn new(service: Arc<LeaderboardService>) -> Self {
        Self { service }
    }

    #[post("/{leaderboard_id}")]
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

    /// Create or update a board from the game's own code.
    ///
    /// Idempotent, so a game can declare its seasons at boot. The path segment is
    /// optional: `POST /v1/leaderboard` takes the id from the body, and
    /// `POST /v1/leaderboard/{id}` lets a body copied from another board keep its
    /// own name.
    #[post("/define")]
    async fn define_body(&self, auth: Authed, Body(body): Body<DefineRequest>) -> Response {
        let state = current_state();
        match self.service.define_board(&auth, "", body).await {
            Ok(value) => axum::Json(value).into_response(),
            Err(err) => err.into_response(),
        }
    }

    #[post("/{leaderboard_id}/define")]
    async fn define(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Body(body): Body<DefineRequest>,
    ) -> Response {
        let state = current_state();
        match self.service.define_board(&auth, &id, body).await {
            Ok(value) => axum::Json(value).into_response(),
            Err(err) => err.into_response(),
        }
    }

    /// Every board the game has defined.
    #[get("")]
    async fn list_boards(&self, auth: Authed) -> Response {
        let state = current_state();
        match self.service.list_boards(&auth).await {
            Ok(value) => axum::Json(value).into_response(),
            Err(err) => err.into_response(),
        }
    }

    /// Remove a board and every score on it.
    #[delete("/{leaderboard_id}/board")]
    async fn delete_board(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        match self.service.delete_board(&auth, &id).await {
            Ok(value) => axum::Json(value).into_response(),
            Err(err) => err.into_response(),
        }
    }

    #[get("/{leaderboard_id}")]
    async fn list_records(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Params(req): Params<ListQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records(&auth, &id, req)
            .await
            .into_response()
    }

    #[delete("/{leaderboard_id}")]
    async fn delete_record(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_record(&auth, &id)
            .await
            .into_response()
    }

    #[get("/{leaderboard_id}/owner/{owner_id}")]
    async fn list_records_around_owner(
        &self,
        auth: Authed,
        Path((id, owner_id)): Path<(String, String)>,
        Params(req): Params<HaystackQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records_around_owner(&auth, &id, &owner_id, req)
            .await
            .into_response()
    }
}
