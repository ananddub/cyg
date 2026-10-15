
use axum::Router;
use axum::routing::get;

use crate::ApiState;
use crate::dashboard::controllers::rest;

pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new()
        .route("/api/dashboard/overview", get(rest::overview))
        .route("/api/dashboard/rooms", get(rest::rooms))
        .route("/api/dashboard/workers", get(rest::workers))
        .route("/api/dashboard/clients", get(rest::clients))
        .route("/api/dashboard/ticks", get(rest::ticks))
}
