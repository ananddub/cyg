
use axum::Router;
use axum::routing::get;

use crate::ApiState;
use crate::dashboard::controllers::ws;

pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new().route("/ws/dashboard", get(ws::upgrade))
}
