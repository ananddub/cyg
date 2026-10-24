use std::path::Path;

use axum::http::StatusCode;

use crate::utils::dashboard::{path_guard, responses};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../dashboard/dist"]
pub struct EmbeddedDashboard;

pub fn serve_index(root: Option<&Path>) -> axum::response::Response {
    if let Some(dist_dir) = root
        && let Ok(body) = std::fs::read(dist_dir.join("index.html"))
    {
        // no-store: a rebuilt shell points at hashed assets a cached page won't have
        return responses::html_response(body, "no-store");
    }
    if let Some(file) = EmbeddedDashboard::get("index.html") {
        return responses::html_response(file.data.into_owned(), "no-store");
    }
    responses::not_built(StatusCode::OK)
}

pub fn serve_asset(root: Option<&Path>, rest: &str) -> axum::response::Response {
    const IMMUTABLE: &str = "public, max-age=31536000, immutable";
    if let Some(dist_dir) = root {
        let path = path_guard::child_under(dist_dir, rest)
            .or_else(|| path_guard::child_under(dist_dir, &format!("assets/{rest}")));
        if let Some(path) = path
            && let Ok(body) = std::fs::read(&path)
        {
            let mime = crate::utils::static_routes::content_type_for(&path.to_string_lossy());
            return responses::file_response(body, mime, IMMUTABLE);
        }
    }
    let clean_rest = rest.trim_start_matches('/');
    if let Some(file) = EmbeddedDashboard::get(clean_rest)
        .or_else(|| EmbeddedDashboard::get(&format!("assets/{clean_rest}")))
    {
        let mime = crate::utils::static_routes::content_type_for(clean_rest);
        return responses::file_response(file.data.into_owned(), mime, IMMUTABLE);
    }
    responses::not_found()
}

#[cfg(test)]
mod tests;
