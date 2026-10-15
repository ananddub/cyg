//! The serve loop. Binding the listener is the control plane's job; this turns
//! a bound listener plus an [`ApiState`] into HTTP.

use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::{ApiState, api};

/// Serve the API until `shutdown` resolves.
pub async fn serve(
    listener: TcpListener,
    state: ApiState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    // `into_make_service_with_connect_info` is what makes the peer socket
    // address reachable as `ConnectInfo<SocketAddr>`; without it every
    // matchmake request fails on a missing request extension and the IP
    // bucket is never charged.
    let router = api::router(&state);
    let _ = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await;
}
