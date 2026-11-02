use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use engine_agones::{AgonesClient, Healthbeat, SidecarConfig};

use crate::harness::{FakeSidecar, closed_port, ok};

#[test]
fn the_ping_loop_survives_refused_connections_and_reports_the_streak() {
    let stop = Arc::new(AtomicBool::new(false));
    let addr = closed_port();
    let handle = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let client = AgonesClient::new(SidecarConfig {
                addr,
                timeout: Duration::from_millis(50),
            });
            let mut beat = Healthbeat::new(client, Duration::from_millis(40));
            beat.run_until_stop(&stop, |_outcome| {});
            beat.stats()
        })
    };

    std::thread::sleep(Duration::from_millis(220));
    stop.store(true, Ordering::Relaxed);
    let stats = handle.join().expect("the beat loop ended cleanly");

    assert!(
        stats.pings >= 3,
        "only {} pings in 220 ms at a 40 ms interval",
        stats.pings
    );
    assert_eq!(stats.pings, stats.failures, "every ping was refused");
    assert_eq!(stats.successes, 0);
    assert_eq!(stats.consecutive_failures, stats.pings as u32);
    assert_eq!(stats.unreachable_failures, stats.pings as u32);
    assert!(
        stats.unhealthy_declared,
        "three misses must be visible in the stats"
    );
}

#[test]
fn the_ping_loop_keeps_beating_a_live_sidecar() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let stop = Arc::new(AtomicBool::new(false));
    let config = fake.config(Duration::from_millis(200));

    let handle = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut beat = Healthbeat::new(AgonesClient::new(config), Duration::from_millis(30));
            beat.run_until_stop(&stop, |_outcome| {});
            beat.stats()
        })
    };

    let seen = fake.wait_for_requests("/health", 4, Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
    let stats = handle.join().expect("the beat loop ended cleanly");

    assert!(seen >= 4, "expected at least 4 heartbeats, saw {seen}");
    assert_eq!(
        stats.failures, 0,
        "a live sidecar must not produce failures"
    );
    assert_eq!(stats.consecutive_failures, 0);
    assert!(stats.pings as usize >= seen);
}
