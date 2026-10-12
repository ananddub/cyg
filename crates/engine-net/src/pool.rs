//! # Multi-Worker Thread-Per-Core Pool
//!
//! Spawns N worker threads (one per CPU core) bound to the same port using `SO_REUSEPORT`.
//! The Linux kernel distributes incoming client TCP connections across the workers with
//! zero lock contention and zero Tokio overhead in the fast game simulation path.

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::worker::{MioWorker, WorkerConfig, WorkerStats};

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

/// Handle to manage running multi-worker pool.
pub struct WorkerPoolHandle {
    running: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    worker_stats: Vec<Arc<WorkerStats>>,
}

impl WorkerPoolHandle {
    /// Stop all worker threads gracefully.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Wait for all worker threads to finish.
    pub fn join(self) -> thread::Result<()> {
        for handle in self.threads {
            handle.join()?;
        }
        Ok(())
    }

    /// Snapshot aggregated telemetry across all active workers.
    pub fn telemetry(&self) -> AggregatedTelemetry {
        let mut total_ccu = 0;
        let mut total_ticks = 0;
        let mut total_packets_in = 0;
        let mut total_packets_out = 0;
        let mut max_tick_time_us = 0;
        let mut sum_tick_time_us = 0;
        let count = self.worker_stats.len();

        for stats in &self.worker_stats {
            total_ccu += stats.ccu.load(Ordering::Relaxed);
            total_ticks += stats.total_ticks.load(Ordering::Relaxed);
            total_packets_in += stats.total_packets_in.load(Ordering::Relaxed);
            total_packets_out += stats.total_packets_out.load(Ordering::Relaxed);

            let t_us = stats.last_tick_time_us.load(Ordering::Relaxed);
            if t_us > max_tick_time_us {
                max_tick_time_us = t_us;
            }
            sum_tick_time_us += t_us;
        }

        let avg_tick_time_us = if count > 0 {
            sum_tick_time_us / count as u64
        } else {
            0
        };

        AggregatedTelemetry {
            total_ccu,
            total_ticks,
            total_packets_in,
            total_packets_out,
            max_tick_time_us,
            avg_tick_time_us,
        }
    }

    /// Access individual per-worker stats for fine-grained per-core monitoring.
    pub fn per_worker_stats(&self) -> &[Arc<WorkerStats>] {
        &self.worker_stats
    }
}

/// Spawns a Thread-Per-Core multi-worker pool.
/// Each thread creates a `MioWorker` bound via `SO_REUSEPORT` and runs its independent 60Hz tick loop.
pub fn spawn_worker_pool<F>(config: PoolConfig, worker_factory: F) -> io::Result<WorkerPoolHandle>
where
    F: Fn(usize) + Send + Sync + 'static + Clone,
{
    let running = Arc::new(AtomicBool::new(true));
    let mut threads = Vec::with_capacity(config.worker_count);
    let mut worker_stats = Vec::with_capacity(config.worker_count);

    for worker_id in 0..config.worker_count {
        let worker_cfg = WorkerConfig {
            bind_addr: config.bind_addr,
            tick_rate: config.tick_rate,
            max_clients: config.max_clients_per_worker,
        };

        let mut worker = MioWorker::new(worker_cfg)?;
        let stats = worker.stats();
        worker_stats.push(stats);

        let thread_running = running.clone();
        let factory = worker_factory.clone();

        let handle = thread::Builder::new()
            .name(format!("mio-worker-{}", worker_id))
            .spawn(move || {
                println!(
                    "⚡ [Worker {}] Thread started & listening on {} (SO_REUSEPORT)",
                    worker_id, config.bind_addr
                );

                while thread_running.load(Ordering::Relaxed) {
                    let res = worker.step(Duration::from_millis(2), |_ctx, _clients| {
                        factory(worker_id);
                    });

                    if let Err(e) = res {
                        eprintln!("[Worker {}] Error during step: {}", worker_id, e);
                        break;
                    }
                }

                println!("✓ [Worker {}] Thread gracefully terminated", worker_id);
            })?;

        threads.push(handle);
    }

    Ok(WorkerPoolHandle {
        running,
        threads,
        worker_stats,
    })
}
