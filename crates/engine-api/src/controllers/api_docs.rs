use auto_route::controller;
use axum::Json;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;

struct ApiDocsController;

#[controller("/openapi.json")]
impl ApiDocsController {
    fn new() -> Self {
        Self
    }

    /// The full spec includes the operator API, so it is console-only.
    #[get("")]
    async fn spec(&self, _auth: ConsoleAuthed) -> Response {
        Json(auto_route::openapi_json()).into_response()
    }
}
