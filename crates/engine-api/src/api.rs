//! The one place the API's route groups are composed, so no group ever edits
//! another's file to add a route.

use axum::Router;

use crate::{ApiState, dashboard, matchmake, signaling, static_routes, status};

/// Every browser-facing route, ready to serve.
pub fn router(state: &ApiState) -> Router {
    Router::new()
        .merge(static_routes::router(state))
        .merge(matchmake::router(state))
        .merge(signaling::router(state))
        .merge(status::router(state))
        .merge(dashboard::api::router(state))
        // One fallback for the whole surface: anything unmatched is a static
        // file request, and `StaticServer` decides whether it exists.
        .fallback(static_routes::fallback)
        .with_state(state.clone())
}
