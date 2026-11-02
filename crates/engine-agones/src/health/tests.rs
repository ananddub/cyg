use std::time::{Duration, Instant};

use super::{Healthbeat, PingStats, next_due_after};
use crate::client::{AgonesClient, SidecarConfig};
use crate::error::SidecarError;

fn stats_with_streak(streak: u32) -> Healthbeat {
    let mut hb = Healthbeat::new(
        AgonesClient::new(SidecarConfig::loopback(1)),
        Duration::from_millis(40),
    );
    hb.stats.consecutive_failures = streak;
    hb.stats.pings = u64::from(streak);
    hb
}

#[test]
fn backoff_doubles_then_caps_at_max_backoff() {
    let i = || Duration::from_millis(40);
    assert_eq!(stats_with_streak(0).current_delay(), i());
    assert_eq!(stats_with_streak(1).current_delay(), i());
    assert_eq!(
        stats_with_streak(2).current_delay(),
        Duration::from_millis(80)
    );
    assert_eq!(
        stats_with_streak(3).current_delay(),
        Duration::from_millis(160)
    );
    assert_eq!(
        stats_with_streak(4).current_delay(),
        Duration::from_millis(320)
    );

    assert_eq!(
        stats_with_streak(40).current_delay(),
        Duration::from_millis(320)
    );
    let capped = stats_with_streak(9).with_max_backoff(Duration::from_millis(100));
    assert_eq!(capped.current_delay(), Duration::from_millis(100));
}

#[test]
fn schedule_absorbs_latency_without_drift() {
    let t0 = Instant::now();
    let interval = Duration::from_millis(50);

    let next = next_due_after(t0, t0 + Duration::from_millis(10), interval);
    assert_eq!(next, t0 + interval);

    let after = next_due_after(next, next + Duration::from_millis(10), interval);
    assert_eq!(after, t0 + interval * 2);
}

#[test]
fn schedule_resyncs_when_a_ping_overruns_and_never_queues_a_burst() {
    let t0 = Instant::now();
    let interval = Duration::from_millis(50);

    let next = next_due_after(t0, t0 + Duration::from_millis(120), interval);
    assert_eq!(next, t0 + Duration::from_millis(170));
    assert!(
        next > t0 + Duration::from_millis(120),
        "the next ping must be in the future"
    );
}

#[test]
fn a_backed_off_streak_delays_the_next_ping_too() {
    let t0 = Instant::now();
    let interval = Duration::from_millis(50);
    let next = next_due_after(t0, t0 + Duration::from_millis(5), interval * 4);
    assert_eq!(next, t0 + interval * 4);
}

#[test]
fn stats_count_by_error_kind_and_declare_unhealthy_once() {
    let mut s = PingStats::default();
    let boom = |kind: &str| SidecarError::Malformed { reason: kind.into() };
    s.note_failure(Duration::from_millis(1), &boom("a"));
    s.note_failure(Duration::from_millis(3), &boom("b"));
    assert!(!s.unhealthy_declared);
    assert_eq!(s.consecutive_failures, 2);
    s.note_failure(
        Duration::from_millis(2),
        &SidecarError::Timeout {
            addr: "127.0.0.1:9358".into(),
            after_ms: 1000,
        },
    );
    assert!(s.unhealthy_declared, "three misses is the Agones window");
    assert_eq!(s.timeout_failures, 1);
    assert_eq!(s.malformed_failures, 2);
    assert_eq!(s.max_consecutive_failures, 3);
    s.note_success(Duration::from_millis(4));
    assert_eq!(s.consecutive_failures, 0, "a recovery resets the streak");
    assert!(s.unhealthy_declared, "but the declaration is history");
    assert_eq!(s.pings, 4);
    assert_eq!(s.mean_latency_ms(), (1 + 3 + 2 + 4) / 4);
}
