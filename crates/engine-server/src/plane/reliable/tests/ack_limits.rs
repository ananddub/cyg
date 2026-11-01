use super::Sink;
use super::wire;
use crate::plane::reliable::*;

#[test]
fn an_event_nobody_acks_is_retired_after_its_lifetime() {
    let mut lane = ReliableLane::new();
    lane.queue(1, wire(1), 0).unwrap();
    lane.queue(2, wire(2), 0).unwrap();
    lane.pump(0, |_| true);

    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    lane.pump(EVENT_LIFETIME, |b| sink.take(b));
    assert_eq!(lane.inflight(), 0);
    assert_eq!(lane.counts().expired, 2);
    assert!(
        sink.sent.is_empty(),
        "an expired event is never transmitted"
    );

    assert!(lane.queue(3, wire(3), EVENT_LIFETIME).is_ok());
    assert_eq!(lane.ack(3, EVENT_LIFETIME + 20), 1);
    assert!(matches!(
        lane.queue(1, wire(1), EVENT_LIFETIME),
        Err(QueueError::NotNewer { seq: 1, last: 3 })
    ));
}

#[test]
fn the_window_fills_and_refuses_instead_of_growing_without_bound() {
    let mut lane = ReliableLane::new();
    for seq in 0..MAX_INFLIGHT_EVENTS as u16 {
        lane.queue(seq, wire(seq), 0).expect("fills the window");
    }
    assert_eq!(lane.inflight(), MAX_INFLIGHT_EVENTS);
    assert_eq!(
        lane.queue(
            MAX_INFLIGHT_EVENTS as u16,
            wire(MAX_INFLIGHT_EVENTS as u16),
            0
        ),
        Err(QueueError::WindowFull {
            inflight: MAX_INFLIGHT_EVENTS
        })
    );
    assert_eq!(lane.counts().rejected, 1);
    assert_eq!(
        lane.counts().queued,
        MAX_INFLIGHT_EVENTS as u64,
        "a refusal does not queue"
    );

    lane.pump(0, |_| true);
    assert_eq!(lane.ack(0, 10), 1);
    assert!(
        lane.queue(
            MAX_INFLIGHT_EVENTS as u16,
            wire(MAX_INFLIGHT_EVENTS as u16),
            10
        )
        .is_ok()
    );
}

#[test]
fn arbitrary_payload_sizes_are_reliable_and_oversize_is_refused() {
    let mut lane = ReliableLane::new();
    let mut seq = 1u16;
    for size in [1usize, 9, 14, 64, 512, 1024, MAX_RELIABLE_BYTES] {
        assert!(
            lane.queue(seq, vec![7u8; size], 0).is_ok(),
            "size {size} must queue"
        );
        seq = seq.wrapping_add(1);
    }
    for bad in [vec![0u8; 0], vec![0u8; MAX_RELIABLE_BYTES + 1]] {
        let len = bad.len();
        assert_eq!(
            lane.queue(seq, bad, 0),
            Err(QueueError::TooBig { bytes: len })
        );
    }
    assert_eq!(lane.counts().rejected, 2);
    assert_eq!(lane.inflight(), 7);
}
