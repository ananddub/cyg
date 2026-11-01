use super::Sink;
use super::wire;
use crate::plane::reliable::*;

#[test]
fn a_queued_event_goes_out_once_and_is_not_resent_before_the_timeout() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };

    lane.queue(7, wire(7), 0).unwrap();
    lane.pump(0, |b| sink.take(b));
    assert_eq!(sink.sent, vec![7], "queue + pump = exactly one datagram");
    assert_eq!(lane.counts().sent, 1);
    assert_eq!(lane.counts().resent, 0);

    lane.pump(lane.rto() / 2, |b| sink.take(b));
    assert_eq!(
        sink.sent.len(),
        1,
        "no resend while the ack is still owed time"
    );
}

#[test]
fn an_unacked_event_is_retransmitted_with_a_bounded_backoff() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    lane.queue(1, wire(1), 0).unwrap();

    let rto = lane.rto();
    lane.pump(0, |b| sink.take(b));
    let mut clock = 0;
    for round in 0..7 {
        clock += rto << round.min(MAX_BACKOFF_SHIFT);
        lane.pump(clock, |b| sink.take(b));
    }
    assert_eq!(
        sink.sent,
        vec![1; 8],
        "one datagram per deadline (the first is the send itself)"
    );
    assert_eq!(lane.counts().sent, 1);
    assert_eq!(lane.counts().resent, 7);

    sink.sent.clear();
    lane.pump(clock + rto, |b| sink.take(b));
    assert!(sink.sent.is_empty());

    assert_eq!(ReliableLane::backoff(rto, 0), rto);
    assert_eq!(ReliableLane::backoff(rto, 99), rto << MAX_BACKOFF_SHIFT);
}

#[test]
fn the_window_is_strictly_ordered_behind_an_un_due_head() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    lane.queue(1, wire(1), 0).unwrap();
    lane.pump(0, |b| sink.take(b));
    assert_eq!(sink.sent, vec![1]);

    lane.queue(2, wire(2), 5).unwrap();
    lane.pump(5, |b| sink.take(b));
    assert_eq!(sink.sent, vec![1], "a backed-off head holds the tail back");

    let head_due = lane.next_due().expect("two events outstanding");
    assert_eq!(head_due, lane.rto());
    lane.pump(head_due, |b| sink.take(b));
    assert_eq!(
        sink.sent,
        vec![1, 1, 2],
        "the head resends, then 2 goes in order"
    );
}

#[test]
fn a_refused_send_leaves_the_event_due_so_the_next_pass_retries_it() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: false,
        ..Sink::default()
    };
    lane.queue(1, wire(1), 0).unwrap();
    lane.pump(0, |b| sink.take(b));
    assert_eq!(
        lane.counts().sent,
        0,
        "a datagram the socket refused is not a send"
    );
    assert_eq!(lane.window[0].attempts, 0, "and it costs no backoff");

    sink.accept = true;
    lane.pump(0, |b| sink.take(b));
    assert_eq!(
        sink.sent,
        vec![1],
        "the next pass transmits it, still due immediately"
    );
    assert_eq!(lane.counts().sent, 1);
    assert_eq!(lane.counts().resent, 0);
}

#[test]
fn sequence_wrap_does_not_fool_the_order_or_the_ack() {
    let mut lane = ReliableLane::new();
    let mut sink = Sink {
        accept: true,
        ..Sink::default()
    };
    for seq in [65534u16, 65535, 0, 1] {
        lane.queue(seq, wire(seq), 0).unwrap();
    }
    lane.pump(0, |b| sink.take(b));
    assert_eq!(sink.sent, vec![65534, 65535, 0, 1]);
    assert_eq!(lane.ack(0, 30), 3);
    assert_eq!(lane.inflight(), 1);
    assert!(matches!(
        lane.queue(65534, wire(65534), 30),
        Err(QueueError::NotNewer { .. })
    ));
}
