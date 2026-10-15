//! # Multi-Worker Thread-Per-Core Pool
//!
//! Spawns N worker threads (one per CPU core) bound to the same port using
//! `SO_REUSEPORT`. The Linux kernel distributes incoming TCP connections across
//! the workers with zero lock contention and zero Tokio overhead in the fast
//! game simulation path.

mod config;
mod handle;
mod spawn;

pub use config::{AggregatedTelemetry, PoolConfig};
pub use handle::WorkerPoolHandle;
pub use spawn::spawn_worker_pool;
