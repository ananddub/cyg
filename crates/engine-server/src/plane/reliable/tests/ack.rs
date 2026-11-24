use super::Sink;
use super::wire;
use crate::plane::reliable::*;

#[test]
fn a_cumulative_ack_releases_the_prefix_and_nothing_is_resent() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    for seq in 1..=5 {
        lane.queue(seq, wire(seq), 0).unwrap();
    }
    lane.pump(0, |b| sink.take(b));
    assert_eq!(sink.sent, vec![1, 2, 3, 4, 5]);

    assert_eq!(lane.ack(4, 60), 4, "ack(4) releases 1..=4");
    assert_eq!(lane.inflight(), 1);
    assert_eq!(lane.counts().acked, 4);

    sink.sent.clear();
    lane.pump(lane.next_due().unwrap() - 1, |b| sink.take(b));
    assert!(
        sink.sent.is_empty(),
        "the new head is not due yet, so 5 stays quiet"
    );
    lane.pump(lane.next_due().unwrap(), |b| sink.take(b));
    assert_eq!(sink.sent, vec![5], "only the still-unacked event is resent");
}

#[test]
fn a_duplicate_or_unrelated_ack_releases_nothing_and_moves_no_estimate() {
    let mut lane = ReliableLane::new();
    lane.queue(3, wire(3), 0).unwrap();
    lane.pump(0, |_| true);
    assert_eq!(lane.ack(3, 50), 1);
    let rtt = lane.rtt_ms();
    assert!(rtt > 0.0, "the first ack must produce a sample");

    assert_eq!(lane.ack(3, 900), 0, "duplicate");
    assert_eq!(lane.ack(9, 950), 0, "window is empty");
    assert_eq!(lane.rtt_ms(), rtt, "a re-delivered ack is not a round trip");
}

#[test]
fn an_ack_of_a_retransmitted_event_does_not_measure_the_round_trip() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    lane.queue(1, wire(1), 0).unwrap();
    lane.pump(0, |b| sink.take(b));
    let resent = lane.rto();
    lane.pump(resent, |b| sink.take(b));
    assert_eq!(lane.counts().resent, 1);

    assert_eq!(lane.ack(1, resent + 5), 1);
    assert_eq!(lane.rtt_ms(), 0.0);
    assert_eq!(lane.rto(), INITIAL_RTO);
}

#[test]
fn a_slow_ack_pulls_the_timeout_toward_the_measured_round_trip() {
    let mut lane = ReliableLane::new();
    for seq in 1..=3 {
        lane.queue(seq, wire(seq), 0).unwrap();
    }
    lane.pump(0, |_| true);

    lane.ack(1, 500);
    assert_eq!(lane.rto(), 1000);

    lane.ack(2, 800);
    assert_eq!(lane.rtt_ms(), 537.5);
    assert_eq!(lane.rto(), 1075, "the timeout followed the estimate");

    assert_eq!(
        ReliableLane::rto_from(1.0),
        MIN_RTO,
        "a 1 ms sample cannot produce a sub-floor timeout"
    );
    assert_eq!(ReliableLane::rto_from(90_000.0), MAX_RTO);
}

#[test]
fn byte_and_pacing_counters_are_reported() {
    let mut lane = ReliableLane::new();
    lane.queue(1, wire(1), 0).unwrap();
    lane.pump(0, |_| true);
    let c = lane.counts();
    assert_eq!(c.bytes_sent, 14);
    assert_eq!(c.sent, 1);
    assert_eq!(c.cwnd_bytes, lane.cwnd_bytes() as u64);
    lane.ack(1, 50);
    let c = lane.counts();
    assert_eq!(c.bytes_acked, 14);
    assert!(c.pto_count == 0);
}
