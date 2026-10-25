//! The one place the API's route groups are composed, so no group ever edits
//! another's file to add a route.

use axum::Router;

use crate::{ApiState, app::state};

/// Every browser-facing route, ready to serve.
///
/// Owns only what a controller cannot: the project-file fallback, a whole-router
/// concern. The route groups register through `auto_route`'s compile-time
/// inventory ([`app`] merges them).
pub fn router(state: &ApiState) -> Router {
    Router::new()
        // One fallback for the whole surface: anything unmatched is a static
        // file request, and `StaticServer` decides whether it exists.
        .fallback(crate::controllers::static_routes::fallback)
        .with_state(state.clone())
}

/// The full surface: the fallback, every auto-registered controller and socket,
/// with the plane state attached per request.
///
/// Tolerant on purpose — a controller or socket that fails to resolve only drops
/// the auto surface, never the hand-written fallback, so one bad DI graph cannot
/// take the API down.
pub async fn app(state: ApiState) -> Router {
    state::install_state(state.clone());
    // The tenancy tables are created here rather than by an operator step: a plane
    // that booted against an empty database still has somewhere to register a game.
    // A failure is reported and ignored — the pool is lazy, so Postgres being down at
    // boot must not stop the API from coming up.
    if let Some(pool) = state.db.as_ref() {
        if let Err(err) = crate::services::games::ensure_schema(pool).await {
            tracing::error!("games: schema bootstrap failed: {err}");
        }
    }
    let mut router = router(&state);
    if let Ok(container) = auto_di::Container::new() {
        match auto_route::build_routes(&container).await {
            Ok(auto) => router = router.merge(auto),
            Err(err) => tracing::info!("auto_route: skipping auto-registered routes: {err}"),
        }
        match auto_ws::mount::<crate::infra::ws::WsProtocol>(&container, Router::new()).await {
            Ok(sockets) => router = router.merge(sockets),
            Err(err) => tracing::info!("auto_ws: skipping auto-registered sockets: {err}"),
        }
    }
    state::with_plane_state(router, state)
}
