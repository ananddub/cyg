//! # Engine Net
//!
//! Ultra-high-performance Thread-Per-Core network transport powered by raw `mio` and `SO_REUSEPORT`:
//! - Kernel-level connection distribution without lock contention
//! - Microsecond epoll wait bounded by 60Hz deterministic game ticks
//! - Length-prefixed binary framing (OpCode + Payload)
//! - Batched socket reads, tick updates, and batched write flushes

pub mod colyseus_state;
pub mod connection;
pub mod framing;
pub mod pool;
pub mod transport;
pub mod worker;

pub use colyseus_state::ColyseusRoomState;
pub use connection::ClientConn;
pub use framing::{OpCode, decode_frame, encode_frame};
pub use pool::{AggregatedTelemetry, PoolConfig, WorkerPoolHandle, spawn_worker_pool};
pub use transport::colyseus_ws::{MatchMakeError, MATCHMAKE_METHODS, close_code, response_status};
pub use transport::{
    ColyseusWsTransport, GameTransport, QuicTransport, RawTcpTransport, TransportEvent,
    TransportType, UdpTransport, create_transport,
};
pub use worker::{ClientPacket, MioWorker, WorkerConfig, WorkerStats};
