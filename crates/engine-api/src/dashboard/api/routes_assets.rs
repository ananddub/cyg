
use axum::Router;
use axum::routing::get;

use crate::ApiState;
use crate::dashboard::controllers::assets;

pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new()
        .route("/dashboard", get(assets::index))
        .route("/dashboard/", get(assets::index))
        .route("/dashboard/favicon.svg", get(assets::favicon))
        .route("/dashboard/icons.svg", get(assets::icons))
        .route("/dashboard/logo.png", get(assets::logo))
        .route("/dashboard/assets/{*rest}", get(assets::asset))
}
