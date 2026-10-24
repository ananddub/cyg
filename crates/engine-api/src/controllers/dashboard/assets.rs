//! `/dashboard*` — the overview page and the files it loads.

use auto_route::controller;
use axum::Router;
use axum::extract::OriginalUri;
use axum::response::Response;

use crate::services::dashboard;
use crate::utils::dashboard::{path_guard, responses};

struct DashboardAssetsController;

#[controller("/dashboard")]
impl DashboardAssetsController {
    fn new() -> Self {
        Self
    }

    #[get("")]
    async fn index(&self) -> Response {
        index().await
    }

    #[get("/favicon.svg")]
    async fn favicon(&self) -> Response {
        favicon().await
    }

    #[get("/icons.svg")]
    async fn icons(&self) -> Response {
        icons().await
    }

    #[get("/logo.png")]
    async fn logo(&self) -> Response {
        logo().await
    }

    #[get("/assets/{*rest}")]
    async fn asset(&self, uri: OriginalUri) -> Response {
        asset(uri).await
    }
}

pub(crate) async fn favicon() -> Response {
    dashboard::assets::serve_asset(path_guard::dist_root().as_deref(), "favicon.svg")
}

pub(crate) async fn icons() -> Response {
    dashboard::assets::serve_asset(path_guard::dist_root().as_deref(), "icons.svg")
}

pub(crate) async fn logo() -> Response {
    dashboard::assets::serve_asset(path_guard::dist_root().as_deref(), "logo.png")
}

pub(crate) async fn index() -> Response {
    dashboard::assets::serve_index(path_guard::dist_root().as_deref())
}

pub(crate) async fn asset(OriginalUri(uri): OriginalUri) -> Response {
    let Some(rest) = uri.path().strip_prefix(path_guard::ASSET_PREFIX) else {
        return responses::not_found();
    };
    dashboard::assets::serve_asset(
        path_guard::dist_root().as_deref(),
        &format!("assets/{rest}"),
    )
}

// `/dashboard/` is the same page as `/dashboard`, but `auto_route` trims a
// controller path's trailing slash, so the alias is registered directly.
#[doc(hidden)]
#[allow(non_snake_case)]
fn __auto_route_factory_dashboard_index_slash<'a>(
    _container: &'a auto_route::__private::auto_di::Container,
) -> auto_route::__private::auto_di::BoxFuture<
    'a,
    Result<Router<()>, auto_route::__private::auto_di::DiError>,
> {
    Box::pin(async move { Ok(Router::new().route("/dashboard/", axum::routing::get(index))) })
}

auto_route::__private::inventory::submit! {
    auto_route::RouteDescriptor::new(__auto_route_factory_dashboard_index_slash)
}

#[cfg(test)]
mod tests;
