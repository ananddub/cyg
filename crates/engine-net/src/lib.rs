//! # Engine Net
//!
//! Ultra-high-performance Thread-Per-Core network transport powered by raw `mio` and `SO_REUSEPORT`:
//! - Kernel-level connection distribution without lock contention
//! - Microsecond epoll wait bounded by 60Hz deterministic game ticks
//! - Length-prefixed binary framing (OpCode + Payload)
//! - Batched socket reads, tick updates, and batched write flushes

pub mod connection;
pub mod framing;
mod bufio;
pub mod pool;
pub mod rtc_plane;
pub mod transport;
pub mod worker;

pub use connection::ClientConn;
pub use bytes;
pub use framing::{OpCode, decode_frame, encode_frame, iter_frames, take_frame};
pub use pool::{AggregatedTelemetry, PoolConfig, WorkerPoolHandle, spawn_worker_pool};
pub use rtc_plane::{
    Plane, PlaneConfig, PlaneHandle, RtcDownlink, RtcInput, RtcPlaneError, UplinkEvent,
};
pub use transport::ws_transport::{MatchMakeError, MATCHMAKE_METHODS, close_code, response_status};
pub use transport::{
    WsTransport, GameTransport, QuicTransport, RawTcpTransport, TransportEvent,
    TransportType, UdpTransport, WebRtcServer, WebRtcServerEvent, create_transport,
};
pub use worker::{ClientPacket, MioWorker, WorkerConfig, WorkerStats};
