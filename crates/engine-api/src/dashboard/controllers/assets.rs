
use axum::extract::OriginalUri;
use axum::response::Response;

use crate::dashboard::services::assets;
use crate::dashboard::utils::path_guard;

pub(crate) async fn favicon() -> Response {
    assets::serve_asset(path_guard::dist_root().as_deref(), "favicon.svg")
}

pub(crate) async fn icons() -> Response {
    assets::serve_asset(path_guard::dist_root().as_deref(), "icons.svg")
}

pub(crate) async fn logo() -> Response {
    assets::serve_asset(path_guard::dist_root().as_deref(), "logo.png")
}

pub(crate) async fn index() -> Response {
    assets::serve_index(path_guard::dist_root().as_deref())
}

pub(crate) async fn asset(OriginalUri(uri): OriginalUri) -> Response {
    let Some(rest) = uri.path().strip_prefix(path_guard::ASSET_PREFIX) else {
        return crate::dashboard::utils::responses::not_found();
    };
    assets::serve_asset(path_guard::dist_root().as_deref(), &format!("assets/{rest}"))
}

#[cfg(test)]
mod tests;
