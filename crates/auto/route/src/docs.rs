use crate::{AutoRouteOpenApi, openapi_json};
use axum::{Json, Router, response::Html, routing::get};
use poem_openapi::OpenApiService;
use scalar_api_reference::axum as scalar;

pub fn openapi_routes(json_path: &'static str, ui_path: &'static str) -> Router<()> {
    Router::new()
        .route(json_path, get(|| async { Json(openapi_json()) }))
        .route(ui_path, get(|| async { Html(swagger_ui_html()) }))
}
pub fn scalar_routes(scalar_path: &'static str, openapi_json_path: &'static str) -> Router<()> {
    let configuration = serde_json::json!({"url": openapi_json_path, "theme": "Deep Space", "layout": "modern", "showSidebar": true, "agent": { "disabled": true }});
    scalar::router(scalar_path, &configuration)
}
fn swagger_ui_html() -> String {
    OpenApiService::new(
        AutoRouteOpenApi,
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    )
    .swagger_ui_html()
}
