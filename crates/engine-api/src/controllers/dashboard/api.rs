//! `/api/dashboard/*` — the JSON endpoints behind the overview page.

use auto_route::controller;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};

use crate::ApiState;
use crate::services::dashboard;

struct DashboardRestController;

#[controller("/api/dashboard")]
impl DashboardRestController {
    fn new() -> Self {
        Self
    }

    #[get("/overview")]
    async fn overview(&self) -> Response {
        overview(State(crate::current_state())).await
    }

    #[get("/rooms")]
    async fn rooms(&self) -> Response {
        rooms(State(crate::current_state())).await
    }

    #[get("/workers")]
    async fn workers(&self) -> Response {
        workers(State(crate::current_state())).await
    }

    #[get("/clients")]
    async fn clients(&self) -> Response {
        clients(State(crate::current_state())).await
    }

    #[get("/ticks")]
    async fn ticks(&self) -> Response {
        ticks(State(crate::current_state())).await
    }
}

pub(crate) async fn overview(State(state): State<ApiState>) -> Response {
    Json(dashboard::build_overview(&state)).into_response()
}

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
