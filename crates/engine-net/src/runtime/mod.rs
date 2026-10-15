//! # Mio Runtime: Thread-Per-Core Event Loop Stack
//!
//! The socket machinery every raw transport is built on: a non-blocking
//! connection with batched read/write buffers ([`ClientConn`]), one mio worker
//! loop bound to a single CPU core ([`MioWorker`]), and the pool that runs N of
//! them on the same port via `SO_REUSEPORT` ([`spawn_worker_pool`]).
//!
//! The kernel spreads accepted connections across the workers, so there is no
//! lock contention and no async-runtime overhead in the tick path: each worker
//! bounds its epoll wait by its own deterministic tick deadline.

pub mod connection;
pub mod pool;
pub mod worker;

pub use connection::ClientConn;
pub use pool::{AggregatedTelemetry, PoolConfig, WorkerPoolHandle, spawn_worker_pool};
pub use worker::{ClientPacket, MioWorker, WorkerConfig, WorkerStats};
