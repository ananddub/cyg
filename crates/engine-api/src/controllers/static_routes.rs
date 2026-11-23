use std::sync::Arc;

use auto_route::controller;
use axum::extract::{OriginalUri, State};
use axum::response::Response;

use crate::ApiState;
use crate::current_state;

use crate::services::static_routes::StaticRoutesService;

struct StaticRoutesController {
    service: Arc<StaticRoutesService>,
}

#[controller("")]
impl StaticRoutesController {
    fn new(service: Arc<StaticRoutesService>) -> Self {
        Self { service }
    }

    #[get("/health")]
    async fn health(&self) -> Response {
        self.service.health()
    }

    #[get("/")]
    async fn root(&self) -> Response {
        let state = current_state();
        self.service.serve_root()
    }

    #[get("/cyg.js")]
    async fn sdk(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk()
    }

    #[get("/cyg_core_rs.wasm")]
    async fn sdk_core_wasm(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk_core_wasm()
    }

    #[get("/openoxide-client.js")]
    async fn sdk_client(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk()
    }

    #[get("/arena_map.json")]
    async fn map(&self) -> Response {
        let state = current_state();
        self.service.serve_map()
    }

    #[get("/packages/sdk/dist/colyseus.js")]
    async fn sdk_bundle(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk_bundle()
    }

    #[get("/node_modules/@colyseus/schema/build/index.mjs")]
    async fn schema(&self) -> Response {
        let state = current_state();
        self.service.serve_schema()
    }

    #[get("/unity")]
    async fn unity(&self) -> Response {
        let state = current_state();
        self.service.serve_unity_index()
    }

    #[get("/unity/{*rest}")]
    async fn unity_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(uri.path())
    }

    #[get("/Build/{*rest}")]
    async fn build_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(uri.path())
    }

    #[get("/TemplateData/{*rest}")]
    async fn template_data_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(uri.path())
    }
}

pub async fn fallback(State(state): State<ApiState>, OriginalUri(uri): OriginalUri) -> Response {
    StaticRoutesService::new(state.plane()).fallback(uri.path())
}
