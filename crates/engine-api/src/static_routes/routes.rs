use axum::Router;
use axum::extract::{OriginalUri, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use super::handlers::{
    file_response, not_found, serve_map, serve_root, serve_sdk, serve_sdk_bundle, serve_schema,
    serve_unity_index,
};
use crate::ApiState;

/// `pub fn router(&ApiState) -> Router<ApiState>` — the frozen entry
/// point `lib.rs` composes. The exact paths the legacy hook answered are
/// registered here; everything else is left to [`fallback`].
pub fn router(state: &ApiState) -> Router<ApiState> {
    let _ = state;
    Router::new()
        .route("/health", get(health))
        .route("/", get(serve_root))
        .route("/openoxide.js", get(serve_sdk))
        .route("/arena_map.json", get(serve_map))
        .route("/packages/sdk/dist/colyseus.js", get(serve_sdk_bundle))
        .route(
            "/node_modules/@colyseus/schema/build/index.mjs",
            get(serve_schema),
        )
        .route("/unity", get(serve_unity_index))
        .route("/unity/", get(serve_unity_index))
        .route("/unity/{*rest}", get(serve_unity_asset))
        // The exported `index.html` references its payload root-absolutely, so
        // the same tree is also reachable at the two prefixes Unity emits.
        .route("/Build/{*rest}", get(serve_unity_asset))
        .route("/TemplateData/{*rest}", get(serve_unity_asset))
}

async fn health() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok")
}

/// Anything unmatched is a project file request: `/tank-3d.html`, `/assets/…`,
/// `…/*.wasm`. Only paths that resolve under the project root are ever opened,
/// and a refusal is a plain 404 — telling the client "no" is enough; explaining
/// that it was a traversal attempt fingerprints the rule to anyone probing.
///
/// A fallback has no matched route, so the path comes from the request URI
/// rather than a `Path` extractor. `Uri::path()` is the raw, still-encoded path
/// and never carries the query string.
pub async fn fallback(
    State(state): State<ApiState>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let statics = &state.statics;
    let path = uri.path();
    match statics.resolve(path).and_then(|p| statics.read(&p)) {
        Some(body) => file_response(path, body),
        None => not_found(),
    }
}

/// Which Unity build-relative path a matched URL asks for, or `None` when the
/// URL is not one of the three Unity prefixes.
///
/// `/unity/…` drops its prefix and `/Build/…`, `/TemplateData/…` keep theirs:
/// the export already lays those two names out as directories under the build
/// folder, so the result joins onto `unity_build` unchanged.
pub(super) fn unity_subpath(path: &str) -> Option<&str> {
    if let Some(sub) = path.strip_prefix("/unity/") {
        return Some(sub);
    }
    if path.starts_with("/Build/") || path.starts_with("/TemplateData/") {
        // Skip the leading slash only; both prefixes are already the directory
        // names Unity wrote into `index.html`.
        return Some(&path[1..]);
    }
    None
}

/// `/unity/{*rest}`, `/Build/{*rest}` and `/TemplateData/{*rest}`, one handler
/// for the three because they name the same tree — see [`unity_subpath`].
///
/// The build root gets its own containment check, so `/Build/../../etc/passwd`
/// reaches neither the project nor the filesystem, and a file the export does
/// not contain is a 404 rather than a directory listing or a panic.
async fn serve_unity_asset(
    State(state): State<ApiState>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let s = &state.statics;
    let Some(sub) = unity_subpath(uri.path()) else {
        return not_found();
    };
    match s.resolve_under(s.unity_build_path(), sub).and_then(|p| s.read(&p)) {
        Some(body) => file_response(sub, body),
        None => not_found(),
    }
}
