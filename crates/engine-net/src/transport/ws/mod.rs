mod accept;
mod client;
mod client_io;
mod error;
mod eventloop;
mod frames;
mod heartbeat;
mod hooks;
mod http;
mod msgpack;
mod protocol;
mod publish;
mod publish_lifecycle;
mod pump;
mod pump_write;
mod rfc6455;
mod transport;
mod transport_api;

/// CYG_T debug-trace gate, resolved once per process — a `var_os` probe per
/// client per flush pass locks + linear-scans the environ thousands of times
/// a second at load.
pub(crate) fn trace_enabled() -> bool {
    static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("CYG_T").is_some())
}

pub use error::MatchMakeError;
pub use heartbeat::HeartbeatConfig;
pub use hooks::HttpHook;
pub use protocol::{MATCHMAKE_METHODS, close_code, response_status};
pub use transport::WsTransport;
