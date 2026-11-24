use axum::http::header;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::StaticRoutesService;
use crate::utils::static_routes::responses::{file_response, not_found};

static EMBEDDED_CYG_JS: &[u8] = include_bytes!("../../../../../sdk/client/web/cyg.js");
static EMBEDDED_CYG_CORE_WASM: &[u8] =
    include_bytes!("../../../../../sdk/client/web/cyg_core_rs.wasm");

impl StaticRoutesService {
    pub fn health(&self) -> Response {
        ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok").into_response()
    }

    pub fn serve_root(&self) -> Response {
        let statics = &self.plane.statics;
        if let Some(body) = statics.read(statics.website_path()) {
            return file_response("index.html", body);
        }
        axum::response::Redirect::temporary("/dashboard/").into_response()
    }

    pub fn serve_sdk(&self) -> Response {
        let s = &self.plane.statics;
        let client = s.project_root().join("cyg-client.js");
        let legacy = s.project_root().join("cyg.js");
        let dist_cyg = if let Some(parent) = s.website_path().parent() {
            parent.join("cyg.js")
        } else {
            std::path::PathBuf::new()
        };
        match s
            .read(&client)
            .or_else(|| s.read(&legacy))
            .or_else(|| s.read(&dist_cyg))
            .or_else(|| s.default_for("/cyg.js"))
        {
            Some(body) => file_response("/cyg.js", body),
            None => file_response("/cyg.js", Arc::new(EMBEDDED_CYG_JS.to_vec())),
        }
    }

    pub fn serve_sdk_core_wasm(&self) -> Response {
        let s = &self.plane.statics;
        let wasm = s.project_root().join("cyg_core_rs.wasm");
        let dist_wasm = if let Some(parent) = s.website_path().parent() {
            parent.join("cyg_core_rs.wasm")
        } else {
            std::path::PathBuf::new()
        };
        match s
            .read(&wasm)
            .or_else(|| s.read(&dist_wasm))
            .or_else(|| s.default_for("/cyg_core_rs.wasm"))
        {
            Some(body) => file_response("/cyg_core_rs.wasm", body),
            None => file_response(
                "/cyg_core_rs.wasm",
                Arc::new(EMBEDDED_CYG_CORE_WASM.to_vec()),
            ),
        }
    }

    pub fn serve_map(&self) -> Response {
        let s = &self.plane.statics;
        let path = s.project_root().join("arena_map.json");
        match s.read(&path) {
            Some(body) => file_response("arena_map.json", body),
            None => not_found(),
        }
    }

    pub fn serve_sdk_bundle(&self) -> Response {
        not_found()
    }

    pub fn serve_schema(&self) -> Response {
        self.serve_schema_mjs()
    }

    pub fn serve_openoxide_client(&self) -> Response {
        not_found()
    }

    pub fn serve_schema_mjs(&self) -> Response {
        let s = &self.plane.statics;
        match s.read(s.schema_mjs_path()) {
            Some(body) => file_response("index.mjs", body),
            None => not_found(),
        }
    }

    pub fn serve_unity_index(&self) -> Response {
        not_found()
    }

    pub fn serve_unity_asset(&self, __path: &str) -> Response {
        not_found()
    }

    pub fn fallback(&self, path: &str) -> Response {
        let statics = &self.plane.statics;
        let p_trimmed = path.trim_start_matches('/');
        if p_trimmed == "website.html"
            || p_trimmed == "game"
            || p_trimmed == "index.html"
            || p_trimmed.is_empty()
        {
            if let Some(body) = statics.read(statics.website_path()) {
                return file_response("index.html", body);
            }
        }
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            return not_found();
        }
        let resolved = statics.resolve(path).filter(|p| p.is_file()).or_else(|| {
            if let Some(parent) = statics.website_path().parent() {
                statics.resolve_under(parent, path).filter(|p| p.is_file())
            } else {
                None
            }
        });
        match resolved.and_then(|p| statics.read(&p)) {
            Some(body) => file_response(path, body),
            None => not_found(),
        }
    }
}
