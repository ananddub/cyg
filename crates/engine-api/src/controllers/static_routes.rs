//! Auto-registered bindings for the project's static surface: the liveness
//! probe, the SDK bundle and map, and the Unity WebGL build.
//!
//! `/unity/` cannot be a `#[controller]` route because `auto_route` trims a
//! path's trailing slash, so it is registered directly below.

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
        self.service.serve_root()
    }

    #[get("/openoxide.js")]
    async fn sdk(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk(&state)
    }

    #[get("/arena_map.json")]
    async fn map(&self) -> Response {
        let state = current_state();
        self.service.serve_map(&state)
    }

    #[get("/packages/sdk/dist/colyseus.js")]
    async fn sdk_bundle(&self) -> Response {
        let state = current_state();
        self.service.serve_sdk_bundle(&state)
    }

    #[get("/node_modules/@colyseus/schema/build/index.mjs")]
    async fn schema(&self) -> Response {
        let state = current_state();
        self.service.serve_schema(&state)
    }

    #[get("/unity")]
    async fn unity(&self) -> Response {
        let state = current_state();
        self.service.serve_unity_index(&state)
    }

    #[get("/unity/{*rest}")]
    async fn unity_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(&state, uri.path())
    }

    // The exported `index.html` references its payload root-absolutely, so the
    // same tree is also reachable at the two prefixes Unity emits.
    #[get("/Build/{*rest}")]
    async fn build_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(&state, uri.path())
    }

    #[get("/TemplateData/{*rest}")]
    async fn template_data_asset(&self, uri: OriginalUri) -> Response {
        let state = current_state();
        self.service.serve_unity_asset(&state, uri.path())
    }
}

/// The whole-surface fallback: unmatched paths are served as project files by the
/// same `StaticServer` the routes use (raw, still-encoded, no query string).
pub async fn fallback(State(state): State<ApiState>, OriginalUri(uri): OriginalUri) -> Response {
    StaticRoutesService.fallback(&state, uri.path())
}

/// `/unity/` is the same page as `/unity`, but `auto_route` trims a controller
/// path's trailing slash, so the alias is registered directly for byte-identical URLs.
#[doc(hidden)]
#[allow(non_snake_case)]
fn __auto_route_factory_unity_index_slash<'a>(
    _container: &'a auto_route::__private::auto_di::Container,
) -> auto_route::__private::auto_di::BoxFuture<
    'a,
    Result<axum::Router<()>, auto_route::__private::auto_di::DiError>,
> {
    Box::pin(async move {
        Ok(axum::Router::new().route("/unity/", axum::routing::get(unity_index_slash)))
    })
}

/// The `/unity/` handler: read the plane, then serve the same page as `/unity`.
async fn unity_index_slash() -> Response {
    let state = current_state();
    StaticRoutesService.serve_unity_index(&state)
}

auto_route::__private::inventory::submit! {
    auto_route::RouteDescriptor::new(__auto_route_factory_unity_index_slash)
}
