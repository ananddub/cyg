
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};

use crate::ApiState;
use crate::dashboard::services;

pub(crate) async fn overview(State(state): State<ApiState>) -> Response {
    Json(services::build_overview(&state)).into_response()
}

pub(crate) async fn rooms(State(state): State<ApiState>) -> Response {
    Json(services::build_rooms(&state)).into_response()
}

pub(crate) async fn workers(State(state): State<ApiState>) -> Response {
    Json(services::build_workers(&state)).into_response()
}

pub(crate) async fn clients(State(state): State<ApiState>) -> Response {
    Json(services::build_clients(&state)).into_response()
}

pub(crate) async fn ticks(State(_state): State<ApiState>) -> Response {
    Json(services::build_ticks()).into_response()
}
