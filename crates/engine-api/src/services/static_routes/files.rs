use axum::http::header;
use axum::response::{IntoResponse, Response};

use crate::ApiState;

use super::StaticRoutesService;
use crate::utils::static_routes::responses::{file_response, not_found};

impl StaticRoutesService {
    pub fn health(&self) -> Response {
        ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok").into_response()
    }

    pub fn serve_root(&self) -> Response {
        axum::response::Redirect::temporary("/dashboard/").into_response()
    }

    pub fn serve_sdk(&self, state: &ApiState) -> Response {
        let s = &state.statics;
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

    pub fn serve_unity_index(&self, _state: &ApiState) -> Response {
        not_found()
    }

    pub fn serve_unity_asset(&self, _state: &ApiState, _path: &str) -> Response {
        not_found()
    }

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
