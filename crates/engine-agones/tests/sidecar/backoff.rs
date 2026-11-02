use std::time::{Duration, Instant};

use engine_agones::{AgonesClient, Healthbeat, SidecarConfig, SidecarErrorKind};

use crate::harness::{FakeSidecar, closed_port, framed, ok};

#[test]
fn backoff_grows_with_the_streak_and_resets_on_recovery() {
    let fake = FakeSidecar::scripted(
        vec![
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
        ],
        ok(b"{}"),
    );
    let interval = Duration::from_millis(40);
    let mut beat = Healthbeat::new(fake.client(Duration::from_millis(200)), interval);

    let first = beat.ping(Instant::now());
    assert!(!first.ok);
    assert_eq!(first.consecutive_failures, 1);
    assert_eq!(
        beat.current_delay(),
        interval,
        "one miss is not worth backing off"
    );

    let second = beat.ping(Instant::now());
    assert_eq!(second.consecutive_failures, 2);
    assert_eq!(beat.current_delay(), interval * 2);

    let third = beat.ping(Instant::now());
    assert_eq!(third.consecutive_failures, 3);
    assert_eq!(beat.current_delay(), interval * 4);
    assert!(
        third.just_missed_the_window,
        "the third miss is the loud one"
    );
    assert_eq!(third.error.unwrap().kind(), SidecarErrorKind::Rejected);

    let fourth = beat.ping(Instant::now());
    assert_eq!(fourth.consecutive_failures, 4);
    assert_eq!(
        beat.current_delay(),
        interval * 8,
        "and the ceiling is eight intervals"
    );
    assert!(
        !fourth.just_missed_the_window,
        "the window is crossed once per streak, not every tick"
    );

    let recovered = beat.ping(Instant::now());
    assert!(recovered.ok, "the scripted 503s are exhausted");
    assert_eq!(recovered.consecutive_failures, 0);
    assert_eq!(
        beat.current_delay(),
        interval,
        "healthy means the plain interval again"
    );

    let stats = beat.stats();
    assert_eq!(stats.rejected_failures, 4);
    assert_eq!(stats.successes, 1);
    assert_eq!(stats.max_consecutive_failures, 4);
    assert!(stats.unhealthy_declared);
}

#[test]
fn the_backoff_ceiling_keeps_recovery_fast() {
    let mut beat = Healthbeat::new(
        AgonesClient::new(SidecarConfig {
            addr: closed_port(),
            timeout: Duration::from_millis(30),
        }),
        Duration::from_millis(50),
    );
    for _ in 0..9 {
        beat.ping(Instant::now());
    }
    assert_eq!(beat.stats().consecutive_failures, 9);
    assert_eq!(
        beat.current_delay(),
        Duration::from_millis(400),
        "8 × the interval, never more"
    );
    assert!(beat.current_delay() <= beat.max_backoff());

    let tuned = Healthbeat::new(
        AgonesClient::new(SidecarConfig {
            addr: closed_port(),
            timeout: Duration::from_millis(20),
        }),
        Duration::from_millis(2000),
    )
    .with_max_backoff(Duration::from_secs(3));
    assert_eq!(tuned.max_backoff(), Duration::from_secs(3));
    assert_eq!(tuned.interval(), Duration::from_millis(2000));
}
