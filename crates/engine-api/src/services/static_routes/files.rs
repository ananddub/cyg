//! The file-serving logic behind the static routes: the liveness probe, the SDK bundle, the
//! arena map, the Unity build and the project-file fallback. Every lookup goes through
//! [`StaticServer`](super::StaticServer), so a refusal or a miss is a plain 404.

use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::ApiState;

use super::StaticRoutesService;
use crate::utils::static_routes::responses::{file_response, not_found};

impl StaticRoutesService {
    /// `/health` — the liveness probe, kept out of the fallback because it must
    /// answer even when the project root has no such file.
    pub fn health(&self) -> Response {
        ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok").into_response()
    }

    /// `/`: redirect to the control dashboard. Engine does not render/inject game websites.
    pub fn serve_root(&self) -> Response {
        axum::response::Redirect::temporary("/dashboard/").into_response()
    }

    /// `/cyg.js`: the project's own copy when it has one, so a game can
    /// override the client glue, else the baked-in example.
    pub fn serve_sdk(&self, state: &ApiState) -> Response {
        let s = &state.statics;
        // naya naam: cyg-client.js (client SDK saaf dikhe); purana
        // cyg.js wale projects fallback me serve hote rahenge
        let client = s.project_root().join("cyg-client.js");
        let legacy = s.project_root().join("cyg.js");
        match s.read(&client).or_else(|| s.read(&legacy)).or_else(|| s.default_for("/cyg.js")) {
            Some(body) => file_response("/cyg.js", body),
            None => not_found(),
        }
    }

    pub fn serve_map(&self, state: &ApiState) -> Response {
        let s = &state.statics;
        let path = s.project_root().join("arena_map.json");
        match s.read(&path) {
            Some(body) => file_response("arena_map.json", body),
            None => not_found(),
        }
    }

    pub fn serve_sdk_bundle(&self, state: &ApiState) -> Response {
        let s = &state.statics;
        match s.read(s.sdk_js_path()) {
            Some(body) => file_response("sdk.js", body),
            None => not_found(),
        }
    }

    pub fn serve_schema(&self, state: &ApiState) -> Response {
        let s = &state.statics;
        match s.read(s.schema_mjs_path()) {
            Some(body) => file_response("index.mjs", body),
            None => not_found(),
        }
    }

    /// `/unity` and `/unity/`: Direct browser game rendering is disabled.
    pub fn serve_unity_index(&self, _state: &ApiState) -> Response {
        not_found()
    }

    /// `/unity/{*rest}`, `/Build/{*rest}` and `/TemplateData/{*rest}`: Direct browser game rendering is disabled.
    pub fn serve_unity_asset(&self, _state: &ApiState, _path: &str) -> Response {
        not_found()
    }

    /// Anything unmatched is a project-file request.
    /// Direct browser game rendering (HTML/HTM) is disabled — OpenOxide operates as a headless game server.
    pub fn fallback(&self, state: &ApiState, path: &str) -> Response {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            return not_found();
        }
        let statics = &state.statics;
        match statics.resolve(path).and_then(|p| statics.read(&p)) {
            Some(body) => file_response(path, body),
            None => not_found(),
        }
    }
}
