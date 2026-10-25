//! Thread-per-core pool spawn: one pinned-ish worker thread per configured core.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::config::PoolConfig;
use super::handle::WorkerPoolHandle;
use crate::runtime::worker::{MioWorker, WorkerConfig};

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
                tracing::info!(
                    "⚡ [Worker {}] Thread started & listening on {} (SO_REUSEPORT)",
                    worker_id, config.bind_addr
                );

                let mut packets = Vec::new();
                while thread_running.load(Ordering::Relaxed) {
                    let res = worker.step(Duration::from_millis(2), &mut packets, |_ctx, _clients| {
                        factory(worker_id);
                    });

                    if let Err(e) = res {
                        tracing::error!("[Worker {}] Error during step: {}", worker_id, e);
                        break;
                    }
                }

                tracing::info!("✓ [Worker {}] Thread gracefully terminated", worker_id);
            })?;

        threads.push(handle);
    }

    Ok(WorkerPoolHandle {
        running,
        threads,
        worker_stats,
    })
}
