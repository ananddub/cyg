//! Handle to a running worker pool: shutdown control and aggregated telemetry.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use super::config::AggregatedTelemetry;
use crate::runtime::worker::WorkerStats;

/// Handle to manage running multi-worker pool.
pub struct WorkerPoolHandle {
    pub(super) running: Arc<AtomicBool>,
    pub(super) threads: Vec<JoinHandle<()>>,
    pub(super) worker_stats: Vec<Arc<WorkerStats>>,
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
