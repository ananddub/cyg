use std::any::Any;
use std::sync::Arc;
use std::sync::RwLock;

use super::ApiState;

pub(super) static INSTALLED_STATE: RwLock<Option<ApiState>> = RwLock::new(None);

tokio::task_local! {
    pub(super) static REQUEST_STATE: ApiState;
    pub(super) static REQUEST_CONTAINER: auto_di::Container;
}

pub fn install_state(state: ApiState) {
    *INSTALLED_STATE
        .write()
        .expect("api state install lock poisoned") = Some(state);
}

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

/// Resolves `T` from the request's DI container, or the global one outside requests.
pub async fn resolve<T>() -> Result<Arc<T>, auto_di::DiError>
where
    T: Any + Send + Sync,
{
    match REQUEST_CONTAINER.try_with(auto_di::Container::clone) {
        Ok(container) => container.resolve::<T>().await,
        Err(_) => auto_di::global_container()?.resolve::<T>().await,
    }
}

pub async fn attach_state(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    match request.extensions().get::<ApiState>().cloned() {
        Some(state) => REQUEST_STATE.scope(state, next.run(request)).await,
        None => next.run(request).await,
    }
}

pub async fn attach_container(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let container = request
        .extensions()
        .get::<Option<auto_di::Container>>()
        .cloned()
        .flatten();
    match container {
        Some(container) => REQUEST_CONTAINER.scope(container, next.run(request)).await,
        None => next.run(request).await,
    }
}

pub fn with_plane_state(
    router: axum::Router,
    state: ApiState,
    container: Option<auto_di::Container>,
) -> axum::Router {
    router
        .layer(axum::middleware::from_fn(
            crate::app::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(attach_state))
        .layer(axum::middleware::from_fn(attach_container))
        .layer(axum::Extension(state))
        .layer(axum::Extension(container))
}

#[doc(hidden)]
pub async fn auto_router(state: ApiState) -> axum::Router {
    crate::app::api::app(state)
        .await
        .expect("boot: schema migrations must succeed")
}
