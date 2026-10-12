//! # API Command Interface
//!
//! The one place game/ops code touches to expose HTTP endpoints. You build an
//! [`Api`], register endpoints with the fluent builder, and hand it to any
//! transport via [`Api::serve`]. The transport version is selected at the last
//! moment — the endpoint definitions never change whether they run over
//! HTTP/1.1 today or HTTP/3 tomorrow.
//!
//! ```no_run
//! use engine_http::{Api, HttpVersion, Request, Response};
//! use std::net::SocketAddr;
//!
//! let api = Api::new()
//!     .get("/health", |_| Response::text("ok"))
//!     .get("/rooms/:id", |req| {
//!         Response::json(format!(r#"{{"room":"{}"}}"#, req.param("id").unwrap_or("?")))
//!     });
//!
//! let addr: SocketAddr = "0.0.0.0:8090".parse().unwrap();
//! let handle = api.serve(HttpVersion::Http1, addr).unwrap();
//! let _ = handle.local_addr();
//! handle.shutdown();
//! ```

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::backend::{self, HttpVersion};
use crate::router::Router;
use crate::server::ServerHandle;
use crate::types::{Method, Request, Response};

/// A transport-agnostic collection of HTTP endpoints.
pub struct Api {
    router: Router,
}

impl Api {
    pub fn new() -> Self {
        Self {
            router: Router::new(),
        }
    }

    pub fn get(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Get, pattern, handler)
    }

    pub fn post(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Post, pattern, handler)
    }

    pub fn put(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Put, pattern, handler)
    }

    pub fn delete(
        self,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.route(Method::Delete, pattern, handler)
    }

    /// Register a handler for an explicit method.
    pub fn route(
        self,
        method: Method,
        pattern: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        Self {
            router: self.router.route(method, pattern, handler),
        }
    }

    /// Number of registered endpoints.
    pub fn endpoint_count(&self) -> usize {
        self.router.len()
    }

    /// Wrap the router for inline dispatch (used by custom backends/tests).
    pub fn into_router(self) -> Arc<Router> {
        Arc::new(self.router)
    }

    /// Dispatch a single request through the router without any socket.
    /// Useful for unit-testing endpoints and for internal RPC-style calls.
    pub fn handle(&self, request: Request) -> Response {
        self.router.dispatch(request)
    }

    /// Start serving on a dedicated background thread using the chosen wire
    /// version. This is the version-switch point: the endpoint set is fixed,
    /// only the transport is selected here.
    pub fn serve(self, version: HttpVersion, bind_addr: SocketAddr) -> io::Result<ServerHandle> {
        backend::serve(version, bind_addr, Arc::new(self.router))
    }

    /// Build the backend without spawning a thread, for inline driving in the
    /// engine tick loop (`while running { backend.poll(tick)? }`).
    pub fn build_backend(
        self,
        version: HttpVersion,
        bind_addr: SocketAddr,
    ) -> io::Result<Box<dyn crate::backend::HttpBackend>> {
        backend::create_backend(version, bind_addr, Arc::new(self.router))
    }
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}
