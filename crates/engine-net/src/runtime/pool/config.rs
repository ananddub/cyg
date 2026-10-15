//! Data models of the worker pool: its knobs and its telemetry snapshot.

use std::net::SocketAddr;
use std::thread;

/// Configuration for the multi-worker thread pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub bind_addr: SocketAddr,
    pub worker_count: usize,
    pub tick_rate: u32,
    pub max_clients_per_worker: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        let cores = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            worker_count: cores,
            tick_rate: 60,
            max_clients_per_worker: 4096,
        }
    }
}

/// Aggregated telemetry snapshot across all running workers.
#[derive(Debug, Clone, Default)]
pub struct AggregatedTelemetry {
    pub total_ccu: usize,
    pub total_ticks: u64,
    pub total_packets_in: u64,
    pub total_packets_out: u64,
    pub max_tick_time_us: u64,
    pub avg_tick_time_us: u64,
}
