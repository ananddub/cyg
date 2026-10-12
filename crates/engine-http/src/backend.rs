//! # HTTP Backend Command Interface
//!
//! The single abstraction the engine drives. Game/endpoint code builds a
//! [`Router`](crate::router::Router); the worker loop calls [`HttpBackend::poll`]
//! exactly like it calls `GameTransport::poll_events`, and every incoming
//! request is dispatched through the router and answered. The transport
//! version (`Http1`, `Http3`, …) is chosen once at construction — the calling
//! code is identical regardless.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

/// Supported HTTP wire versions. Adding `Http2`/`Http3` later means adding a
/// new backend implementing [`HttpBackend`]; nothing else changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    /// HTTP/1.1 over raw non-blocking mio TCP.
    Http1,
    /// Reserved: HTTP/3 over the QUIC transport. Not yet implemented.
    Http3,
}

impl HttpVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http1 => "http1",
            Self::Http3 => "http3",
        }
    }
}

impl std::str::FromStr for HttpVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http1" | "http/1.1" | "1.1" | "h1" => Ok(Self::Http1),
            "http3" | "http/3" | "h3" | "quic" => Ok(Self::Http3),
            _ => Err(format!("Unknown HTTP version: {}", s)),
        }
    }
}

/// A protocol backend driven by the engine tick loop.
pub trait HttpBackend: Send {
    /// Drive the event loop for up to `timeout`, serving any requests that
    /// arrive. Returns the number of requests served during this call.
    fn poll(&mut self, timeout: Duration) -> io::Result<usize>;

    /// The socket the backend is listening on.
    fn local_addr(&self) -> io::Result<SocketAddr>;

    /// The wire protocol this backend speaks.
    fn version(&self) -> HttpVersion;

    /// Monotonic count of requests served since construction.
    fn requests_served(&self) -> u64;

    /// Convenience loop: run the backend on the current thread until `running`
    /// goes false. Each iteration serves whatever arrives within `tick`.
    fn run_until_stopped(
        &mut self,
        running: &std::sync::atomic::AtomicBool,
        tick: Duration,
    ) -> io::Result<()> {
        use std::sync::atomic::Ordering;
        while running.load(Ordering::Relaxed) {
            self.poll(tick)?;
        }
        Ok(())
    }
}

use std::sync::Arc;

use crate::router::Router;
use crate::server::Http1Backend;

/// Construct the backend for the requested wire version. This is the only
/// place that knows which concrete transports exist — the engine's entry point
/// for "which protocol should the API speak".
pub fn create_backend(
    version: HttpVersion,
    bind_addr: SocketAddr,
    router: Arc<Router>,
) -> io::Result<Box<dyn HttpBackend>> {
    match version {
        HttpVersion::Http1 => Ok(Box::new(Http1Backend::new(bind_addr, router)?)),
        HttpVersion::Http3 => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HTTP/3 backend is not implemented yet; bind the QUIC transport or use http1",
        )),
    }
}

/// Bind and run a backend on a dedicated OS thread until the returned
/// [`ServerHandle`] is dropped or stopped.
pub fn serve(
    version: HttpVersion,
    bind_addr: SocketAddr,
    router: Arc<Router>,
) -> io::Result<crate::server::ServerHandle> {
    let backend = create_backend(version, bind_addr, router)?;
    crate::server::ServerHandle::spawn(backend)
}
