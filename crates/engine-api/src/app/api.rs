use axum::Router;

use crate::{ApiState, app::state};

/// The base every versioned route is served under. The version literal lives
/// here and nowhere else: controllers are authored at `/v1/...`, the macro strips
/// that segment when it registers them, and this is the mount that puts it back.
///
/// There is no second base. A route that is not under `/v1` is either an
/// operational surface (`/status`, `/metrics`, `/ws/*`, `/dashboard/*`,
/// `/lobby`, `/queue`, `/matchmake`, `/openapi.json`, `/auto/*`) or a bug.
pub const API_BASE: &str = "/v1";

pub fn router(state: &ApiState) -> Router {
    Router::new()
        .nest_service(
            "/console/api-docs",
            auto_route::scalar_routes("/console/api-docs", "/openapi.json"),
        )
        .fallback(crate::controllers::static_routes::fallback)
        .with_state(state.clone())
}

pub async fn app(state: ApiState) -> Result<Router, crate::db::DbError> {
    state::install_state(state.clone());
    // The spec publishes the canonical base, whatever the attributes spell.
    auto_route::set_api_base(API_BASE);
    if let Some(pool) = state.db.as_ref() {
        if let Err(err) = crate::services::games::ensure_schema(pool).await {
            tracing::error!("games: schema bootstrap failed: {err}");
        }
        let steps =
            crate::db::migrations::apply_migrations(pool, crate::db::migrations::MIGRATIONS)
                .await
                .inspect_err(|err| {
                    tracing::error!("boot aborted: schema migrations failed: {err}")
                })?;
        if !steps.is_empty() {
            tracing::info!("[migrations] {} step(s) applied", steps.len());
        }
        match crate::services::games::tenant::resolve(&state.plane()).await {
            Ok(Some(game)) => {
                tracing::info!("[tenancy] hosting '{}' ({})", game.name, game.game_key);
                crate::services::games::tenant::backfill(&state.plane(), game.id).await;
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
        let plane = state.plane();
        register_plane(container, &state, &plane);
        match auto_route::build_root_routes(container).await {
            Ok(auto) => router = router.merge(auto),
            Err(err) => {
                eprintln!("DIAG root: {err}");
                tracing::info!("auto_route: skipping auto-registered routes: {err}")
            }
        }
        match auto_route::build_versioned_routes(container).await {
            Ok(versioned) => router = router.nest(API_BASE, versioned),
            Err(err) => {
                eprintln!("DIAG versioned: {err}");
                tracing::info!("auto_route: skipping versioned routes: {err}")
            }
        }
        match auto_ws::mount::<crate::infra::ws::WsProtocol>(container, Router::new()).await {
            Ok(sockets) => router = router.merge(sockets),
            Err(err) => tracing::info!("auto_ws: skipping auto-registered sockets: {err}"),
        }
    }
    let router = state::with_plane_state(router, state, container);
    // Custom methods are authored as `/resource/{id}:verb`; this adapter
    // rewrites them to the nested Axum route before matching.
    Ok(axum::Router::new()
        .fallback_service(crate::app::canonical_paths::CanonicalPaths::new(router)))
}

/// Everything a service may inject instead of reaching into `&ApiState`.
///
/// The two optional pieces (`Queries`, `BlobStore`) are registered only when a
/// database is configured: a controller that resolves them then fails to mount,
/// which is why the services that need them keep the `deps::q(state)` accessor
/// and its "no database" error. Everything else is always present.
fn register_plane(
    container: &auto_di::Container,
    state: &ApiState,
    plane: &std::sync::Arc<crate::services::plane::Plane>,
) {
    let _ = container.register_instance(plane.clone());
    let _ = container.register_instance(state.control.clone());
    let _ = container.register_instance(state.game.clone());
    let _ = container.register_instance(state.bus.clone());
    let _ = container.register_instance(state.seats.clone());
    let _ = container.register_instance(state.limiter.clone());
    let _ = container.register_instance(std::sync::Arc::new(state.signals.clone()));
    let _ = container.register_instance(state.statics.clone());
    let _ = container.register_instance(state.sessions.clone());
    let _ = container.register_instance(state.realtime.clone());
    let _ = container.register_instance(state.chain.clone());
    let _ = container.register_instance(std::sync::Arc::new(plane.store.clone()));
    if let Some(queries) = state.q.clone() {
        let _ = container.register_instance(queries);
    }
    if let Some(blobs) = state.blobs.clone() {
        let _ = container.register_instance(blobs);
    }
}
