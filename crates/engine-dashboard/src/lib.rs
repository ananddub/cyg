//! # Engine Dashboard
//!
//! Real-time telemetry, server metrics, and embedded admin panel.

use std::time::Duration;

/// Telemetry metrics snapshot collected across active rooms and connections.
#[derive(Debug, Default, Clone)]
pub struct ServerTelemetry {
    pub ccu: usize,
    pub active_rooms: usize,
    pub p99_tick_time: Duration,
    pub total_packets_in: u64,
    pub total_packets_out: u64,
}
