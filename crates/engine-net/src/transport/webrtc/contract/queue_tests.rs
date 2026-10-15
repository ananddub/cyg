//! Drop-oldest backpressure over one lane's send queue.

use std::collections::VecDeque;

use super::fixture::frame;
use super::{MAX_QUEUED_FRAMES, OutboundDelta, OutboundQueue};

#[test]
fn backpressure_drops_the_oldest_delta_whole() {
    let mut queue = OutboundQueue::default();
    let one = |seq: u16| OutboundDelta {
        seq: Some(seq),
        tick: Some(seq as u32),
        frames: VecDeque::from(vec![frame(seq, 1, 0, 1, &[seq as u8; 100]); 1]),
    };

    // Fill to capacity with single-frame deltas.
    for s in 0..MAX_QUEUED_FRAMES as u16 {
        assert_eq!(queue.push(one(s), MAX_QUEUED_FRAMES), (0, 0));
    }
    assert_eq!(queue.frames(), MAX_QUEUED_FRAMES);
    assert_eq!(queue.dropped_deltas(), 0);

    // One more evicts exactly the oldest.
    assert_eq!(queue.push(one(2000), MAX_QUEUED_FRAMES), (1, 1));
    assert_eq!(queue.dropped_deltas(), 1);
    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(queue.frames(), MAX_QUEUED_FRAMES);
    assert_eq!(queue.peek().unwrap().seq, Some(1), "newest wins");
    assert_eq!(queue.items.iter().map(|d| d.seq.unwrap()).max().unwrap(), 2000);
}

/// A whole delta is evicted rather than half of it, so the receiver is not
/// left holding the tail of a message whose head already left.
#[test]
fn a_multi_frame_delta_is_evicted_as_one_unit() {
    let mut queue = OutboundQueue::default();
    let fat = OutboundDelta {
        seq: Some(1),
        tick: Some(1),
        frames: VecDeque::from(vec![frame(1, 1, 0, 2, &[0u8; 10]); 2]),
    };
    assert_eq!(queue.push(fat, 3), (0, 0));
    assert_eq!(queue.frames(), 2);

    let newer = OutboundDelta {
        seq: Some(2),
        tick: Some(2),
        frames: VecDeque::from(vec![frame(2, 2, 0, 1, &[0u8; 10]); 2]),
    };
    assert_eq!(queue.push(newer, 3), (1, 2), "the older delta left entirely");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.frames(), 2);
    assert_eq!(queue.peek().unwrap().seq, Some(2));
    assert_eq!(queue.dropped_frames(), 2);
}

#[test]
fn a_delta_bigger_than_the_queue_is_refused_rather_than_evicting_everything() {
    let mut queue = OutboundQueue::default();
    queue.push(
        OutboundDelta {
            seq: Some(1),
            tick: Some(1),
            frames: VecDeque::from(vec![frame(1, 1, 0, 1, &[0u8; 4]); 1]),
        },
        8,
    );
    let too_big = OutboundDelta {
        seq: Some(2),
        tick: Some(2),
        frames: VecDeque::from(vec![frame(2, 2, 0, 9, &[0u8; 4]); 9]),
    };
    assert_eq!(queue.push(too_big, 8), (1, 9));
    assert_eq!(queue.len(), 1, "the queued delta survived");
    assert_eq!(queue.peek().unwrap().seq, Some(1));
    assert_eq!(queue.dropped_deltas(), 1);
}

/// Supersession discards the deltas a newer tick replaced, from the front in,
/// and stops at the first one that is still newer.
#[test]
fn superseded_deltas_leave_the_queue_oldest_first() {
    let mut queue = OutboundQueue::default();
    for s in [10u16, 11, 12] {
        queue.push(
            OutboundDelta {
                seq: Some(s),
                tick: Some(s as u32),
                frames: VecDeque::from(vec![frame(s, 1, 0, 1, &[0u8; 4])]),
            },
            8,
        );
    }
    assert_eq!(queue.purge_older_than(11), 2, "10 and 11 itself were both replaced");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.peek().unwrap().seq, Some(12));
    assert_eq!(queue.frames(), 1);

    // A raw write with no seq has not been superseded by anything.
    queue.push_front(OutboundDelta {
        seq: None,
        tick: None,
        frames: VecDeque::from(vec![frame(12, 1, 0, 1, &[0u8; 4])]),
    });
    assert_eq!(queue.purge_older_than(99), 0);
    assert_eq!(queue.frames(), 2);
}

/// Wrap: 0 is newer than 65535, so a delta at 0 supersedes it.
#[test]
fn supersession_is_wrap_safe() {
    let mut queue = OutboundQueue::default();
    queue.push(
        OutboundDelta {
            seq: Some(u16::MAX),
            tick: Some(1),
            frames: VecDeque::from(vec![frame(u16::MAX, 1, 0, 1, &[0u8; 4])]),
        },
        8,
    );
    assert_eq!(queue.purge_older_than(0), 1);
    assert!(queue.is_empty());
}

#[test]
fn popping_drains_in_fifo_order_and_clearing_counts_frames() {
    let mut queue = OutboundQueue::default();
    for s in 1..=3u16 {
        queue.push(
            OutboundDelta {
                seq: Some(s),
                tick: Some(s as u32),
                frames: VecDeque::from(vec![frame(s, 1, 0, 1, &[s as u8; 8])]),
            },
            8,
        );
    }
    assert_eq!(queue.pop().unwrap().seq, Some(1));
    assert_eq!(queue.frames(), 2);
    assert_eq!(queue.clear(), 2);
    assert!(queue.is_empty());
    assert_eq!(queue.frames(), 0);
    assert_eq!(queue.pop(), None);
}
