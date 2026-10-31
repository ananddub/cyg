use auto_route::controller;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ApiState;
use crate::app::params::Params;
use crate::error::V2Error;
use crate::services::dashboard;
use crate::services::games::tenant;

struct DashboardRestController;

#[derive(Debug, Default, Deserialize)]
struct DashboardScope {
    #[serde(default)]
    game_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct DashboardIdentity {
    game_id: Option<Uuid>,
    game_name: Option<String>,
}

#[controller("/api/dashboard")]
impl DashboardRestController {
    fn new() -> Self {
        Self
    }

    #[get("/identity")]
    async fn identity(&self) -> Response {
        let state = crate::current_state();
        match tenant::resolve(&state).await {
            Ok(Some(game)) => Json(DashboardIdentity {
                game_id: Some(game.id),
                game_name: Some(game.name),
            })
            .into_response(),
            Ok(None) => Json(DashboardIdentity {
                game_id: None,
                game_name: None,
            })
            .into_response(),
            Err(err) => err.into_response(),
        }
    }

    #[get("/overview")]
    async fn overview(&self, Params(scope): Params<DashboardScope>) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        overview(State(state)).await
    }

    #[get("/rooms")]
    async fn rooms(&self, Params(scope): Params<DashboardScope>) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        let rows = dashboard::build_rooms_scoped(&state, scope.game_id).await;
        Json(rows).into_response()
    }

    #[get("/workers")]
    async fn workers(&self, Params(scope): Params<DashboardScope>) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        workers(State(state)).await
    }

    #[get("/clients")]
    async fn clients(&self, Params(scope): Params<DashboardScope>) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        clients(State(state)).await
    }

    #[get("/ticks")]
    async fn ticks(&self, Params(scope): Params<DashboardScope>) -> Response {
        let state = crate::current_state();
        if let Err(err) = guard(&state, scope.game_id).await {
            return err.into_response();
        }
        ticks(State(state)).await
    }
}

pub(crate) async fn guard(state: &ApiState, claimed: Option<Uuid>) -> Result<(), V2Error> {
    match claimed {
        Some(game_id) => tenant::require_hosted(state, game_id).await,
        None => Ok(()),
    }
}

pub(crate) async fn overview(State(state): State<ApiState>) -> Response {
    Json(dashboard::build_overview(&state)).into_response()
}

#[allow(dead_code)]
pub(crate) async fn rooms(State(state): State<ApiState>) -> Response {
    Json(dashboard::build_rooms(&state)).into_response()
}

pub(crate) async fn workers(State(state): State<ApiState>) -> Response {
    Json(dashboard::build_workers(&state)).into_response()
}

pub(crate) async fn clients(State(state): State<ApiState>) -> Response {
    Json(dashboard::build_clients(&state)).into_response()
}

pub(crate) async fn ticks(State(_state): State<ApiState>) -> Response {
    Json(dashboard::build_ticks()).into_response()
}
