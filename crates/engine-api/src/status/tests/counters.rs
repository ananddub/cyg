use super::*;
use crate::status::view::{age_ms, status};

/// Seats and rate-limit counters are the two numbers an operator watches when
/// a room "won't let anyone in", so they must move with the real registries
/// rather than being decorative.
#[tokio::test]
async fn counters_track_the_registries_they_report() {
    let s = state();
    s.seats.lock().unwrap().mint("tank-3d", 300);
    assert!(s.seats.lock().unwrap().verify("nonsense").is_none(), "forged token rejected");
    let ip = std::net::IpAddr::from([127, 0, 0, 1]);
    for _ in 0..41 {
        s.limiter.lock().unwrap().allow(ip);
    }
    // Held: `SeatReplies` detaches the route when it drops, so a temporary
    // here would leave `peers` back at zero before `/status` reads it.
    let _replies = s.signals.attach("seat-1");

    let body = json_of(status(State(s.clone())).await).await;
    assert_eq!(body["seats"]["minted"].as_u64(), Some(1));
    assert_eq!(body["seats"]["outstanding"].as_u64(), Some(1));
    assert_eq!(body["seats"]["rejected"].as_u64(), Some(1));
    assert_eq!(body["rateLimit"]["allowed"].as_u64(), Some(40), "burst is 40");
    assert_eq!(body["rateLimit"]["denied"].as_u64(), Some(1));
    assert_eq!(body["rateLimit"]["tracked"].as_u64(), Some(1));
    assert_eq!(body["rateLimit"]["rps"].as_u64(), Some(20));
    assert_eq!(body["signaling"]["peers"].as_u64(), Some(1));
}

/// `age_ms` is measured against the sim's own monotonic epoch, so a stale
/// sample ages and a fresh one does not.
#[tokio::test]
async fn age_ms_uses_the_sims_monotonic_clock() {
    let stamp = mono_ns() / 1_000_000;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let aged = StatusSnapshot { updated_ms: stamp, ..Default::default() };
    let age = age_ms(&aged);
    assert!((60..2_000).contains(&age), "a 60 ms-old stamp read as {age} ms");

    // A sample stamped in the future must read as zero age, never as a wrap.
    let ahead = StatusSnapshot { updated_ms: stamp + 1_000_000, ..Default::default() };
    assert_eq!(age_ms(&ahead), 0);
}
