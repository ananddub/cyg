
pub mod routes_assets;
pub mod routes_rest;
pub mod routes_ws;

use axum::Router;

use crate::ApiState;

pub fn router(state: &ApiState) -> Router<ApiState> {
    routes_rest::router(state).merge(routes_ws::router(state)).merge(routes_assets::router(state))
}
