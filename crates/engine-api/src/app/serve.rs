use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::ApiState;

pub async fn serve(
    listener: TcpListener,
    state: ApiState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    let router = crate::app::api::app(state).await;
    let _ = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await;
}
