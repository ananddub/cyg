//! Route table for the telemetry endpoints.

use axum::Router;
use axum::routing::get;

use crate::ApiState;

use super::rooms::rooms;
use super::view::status;

/// `pub fn router(&ApiState) -> Router<ApiState>` — frozen entry point.
pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new()
        .route("/status", get(status))
        .route("/api/rooms", get(rooms))
}
