//! Auto-registered bindings for `/v2/leaderboard/*`.
//!
//! Methods return `Response` (not `Json<T>`) so `auto_route` does not require
//! `T: poem_openapi::Type`; the wire types here are plain `serde` and the
//! response bytes are unchanged.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::current_state;
use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};

use crate::dto::leaderboard::{HaystackQuery, ListQuery};
use crate::services::leaderboard::LeaderboardService;

struct LeaderboardController {
    service: Arc<LeaderboardService>,
}

#[controller("/v2/leaderboard")]
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
            .write_record(&state, &auth, &id, body)
            .await
            .into_response()
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
            .list_records(&state, &auth, &id, req)
            .await
            .into_response()
    }

    #[delete("/{leaderboard_id}")]
    async fn delete_record(&self, auth: Authed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_record(&state, &auth, &id)
            .await
            .into_response()
    }

    #[get("/{leaderboard_id}/owner/{owner_id}")]
    async fn list_records_around_owner(
        &self,
        auth: Authed,
        Path(id): Path<String>,
        Path(owner_id): Path<String>,
        Params(req): Params<HaystackQuery>,
    ) -> Response {
        let state = current_state();
        self.service
            .list_records_around_owner(&state, &auth, &id, &owner_id, req)
            .await
            .into_response()
    }
}
