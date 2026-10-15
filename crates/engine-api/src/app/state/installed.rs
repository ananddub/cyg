//! Process-wide and request-scoped access to the plane's [`ApiState`].

use std::sync::RwLock;

use super::ApiState;

/// The plane's [`ApiState`], installed once at serve time. `auto-di` builds controllers
/// through their zero-dependency `new()`, so a controller reads [`current_state`]:
/// request-scoped state if present, else this global, which only direct unit tests rely on.
pub(super) static INSTALLED_STATE: RwLock<Option<ApiState>> = RwLock::new(None);

tokio::task_local! {
    /// The plane serving the request being handled. One value per request is what
    /// keeps concurrent tests with different planes from fighting over a single
    /// process-wide slot.
    pub(super) static REQUEST_STATE: ApiState;
}

/// Install the state auto-registered controllers see when no request-scoped state
/// is attached. Replaces any previous state, so each test gets its own plane.
pub fn install_state(state: ApiState) {
    *INSTALLED_STATE
        .write()
        .expect("api state install lock poisoned") = Some(state);
}

/// The state serving the current request: attached by [`with_plane_state`], or —
/// for a unit test calling a handler directly — the last [`install_state`].
pub fn current_state() -> ApiState {
    REQUEST_STATE
        .try_with(|state| state.clone())
        .unwrap_or_else(|_| {
            INSTALLED_STATE
                .read()
                .expect("api state install lock poisoned")
                .clone()
                .expect("ApiState installed before the first auto-registered request")
        })
}

/// Attach the plane's state to a request: `Extension<ApiState>` for extractors, and the
/// task-local scope [`current_state`] reads. A middleware rather than a router `S`
/// parameter because the auto-registered controllers are merged into a `Router<()>` —
/// they cannot take `State<ApiState>`.
pub async fn attach_state(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    match request.extensions().get::<ApiState>().cloned() {
        Some(state) => REQUEST_STATE.scope(state, next.run(request)).await,
        None => next.run(request).await,
    }
}

/// Give an already-composed router the plane state — call this after the last `merge`,
/// so every route in the final surface is covered.
pub fn with_plane_state(router: axum::Router, state: ApiState) -> axum::Router {
    router
        .layer(axum::middleware::from_fn(attach_state))
        .layer(axum::Extension(state))
}

/// Install `state`, then resolve every auto-registered controller through a fresh
/// `auto-di` container and merge their routers — [`crate::app::serve`] minus the
/// hand-written groups.
#[doc(hidden)]
pub async fn auto_router(state: ApiState) -> axum::Router {
    crate::app::api::app(state).await
}
