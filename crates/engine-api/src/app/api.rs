use axum::Router;

use crate::{ApiState, app::state};

pub fn router(state: &ApiState) -> Router {
    Router::new()
        .fallback(crate::controllers::static_routes::fallback)
        .with_state(state.clone())
}

pub async fn app(state: ApiState) -> Router {
    state::install_state(state.clone());
    if let Some(pool) = state.db.as_ref() {
        if let Err(err) = crate::services::games::ensure_schema(pool).await {
            tracing::error!("games: schema bootstrap failed: {err}");
        }
        crate::services::games::ensure_tenant_schema(pool).await;
        match crate::services::games::tenant::resolve(&state).await {
            Ok(Some(game)) => {
                tracing::info!(
                    "[tenancy] hosting '{}' ({})",
                    game.name,
                    game.game_key
                );
                crate::services::games::tenant::backfill(&state, game.id).await;
            }
            Ok(None) => tracing::warn!(
                "[tenancy] no hosted game resolved; set game_id/game_key in config.toml"
            ),
            Err(err) => {
                tracing::error!("[tenancy] identity resolution failed: {}", err.message)
            }
        }
    }
    let mut router = router(&state);
    let container = auto_di::Container::new().ok();
    if let Some(container) = &container {
        register_plane(container, &state);
        match auto_route::build_routes(container).await {
            Ok(auto) => router = router.merge(auto),
            Err(err) => tracing::info!("auto_route: skipping auto-registered routes: {err}"),
        }
        match auto_ws::mount::<crate::infra::ws::WsProtocol>(container, Router::new()).await {
            Ok(sockets) => router = router.merge(sockets),
            Err(err) => tracing::info!("auto_ws: skipping auto-registered sockets: {err}"),
        }
    }
    state::with_plane_state(router, state, container)
}

fn register_plane(container: &auto_di::Container, state: &ApiState) {
    if let Some(queries) = state.q.clone() {
        let _ = container.register_instance(queries);
    }
    if let Some(blobs) = state.blobs.clone() {
        let _ = container.register_instance(blobs);
    }
}
