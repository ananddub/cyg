use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::mime::content_type_for;
use crate::ApiState;

pub(super) fn file_response(path: &str, body: Arc<Vec<u8>>) -> Response {
    // `no-cache` rather than `immutable`: a page under development reloads
    // against a rebuilt bundle, and a stale SDK copy is the classic "why is it
    // still broken". The server side is already re-stamped per request, so the
    // browser revalidating costs a stat, not a read.
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type_for(path)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, must-revalidate"),
    );
    (headers, body.to_vec()).into_response()
}

pub(super) fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, header::HeaderValue::from_static("text/plain; charset=utf-8"))],
        "404 not found",
    )
        .into_response()
}

/// `/openoxide.js`: the project's own copy when it has one, so a game can
/// override the client glue, else the baked-in example.
pub(super) async fn serve_sdk(State(state): State<ApiState>) -> Response {
    let s = &state.statics;
    let path = s.project_root().join("openoxide.js");
    match s.read(&path).or_else(|| s.default_for("/openoxide.js")) {
        Some(body) => file_response("/openoxide.js", body),
        None => not_found(),
    }
}

/// `/`: redirect to the control dashboard. Engine does not render/inject game websites.
pub(super) async fn serve_root() -> Response {
    axum::response::Redirect::temporary("/dashboard/").into_response()
}

pub(super) async fn serve_map(State(state): State<ApiState>) -> Response {
    let s = &state.statics;
    let path = s.project_root().join("arena_map.json");
    match s.read(&path) {
        Some(body) => file_response("arena_map.json", body),
        None => not_found(),
    }
}

pub(super) async fn serve_sdk_bundle(State(state): State<ApiState>) -> Response {
    let s = &state.statics;
    match s.read(s.sdk_js_path()) {
        Some(body) => file_response("sdk.js", body),
        None => not_found(),
    }
}

pub(super) async fn serve_schema(State(state): State<ApiState>) -> Response {
    let s = &state.statics;
    match s.read(s.schema_mjs_path()) {
        Some(body) => file_response("index.mjs", body),
        None => not_found(),
    }
}

/// `/unity` and `/unity/`: the build's own landing page.
pub(super) async fn serve_unity_index(State(state): State<ApiState>) -> Response {
    let s = &state.statics;
    match s.read(&s.unity_build_path().join("index.html")) {
        Some(body) => file_response("index.html", body),
        None => not_found(),
    }
}
