use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::ApiState;

pub async fn serve(
    listener: TcpListener,
    state: ApiState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    // Facts are queued by the requests that make them true; this is the half that
    // owes them to somebody else's receiver. It stops with the server, because a
    // process that is no longer answering HTTP has no business sending either.
    let outbox = tokio::spawn(crate::services::webhook::delivery_loop(state.clone()));
    let router = match crate::app::api::app(state).await {
        Ok(router) => router,
        Err(err) => {
            tracing::error!("[boot] refusing to serve with a half-migrated schema: {err}");
            std::process::exit(1);
        }
    };
    let _ = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await;
    outbox.abort();
}
