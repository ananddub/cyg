use super::*;

#[test]
fn publish_to_an_unknown_seat_is_refused_and_counted() {
    let hub = SignalHub::new("[]".into());
    assert!(!hub.publish(
        "nobody",
        SignalOut::Error {
            message: "x".into()
        }
    ));
    assert_eq!(hub.counts(), (0, 1));
    assert_eq!(hub.peers(), 0);
}

#[test]
fn a_full_per_seat_queue_drops_and_counts_without_blocking_the_publisher() {
    let hub = SignalHub::new("[]".into());
    let mut replies = hub.attach("seat");
    for i in 0..PER_SEAT_QUEUE {
        assert!(hub.publish("seat", SignalOut::Answer { sdp: i.to_string() }));
    }
    assert_eq!(hub.counts(), (PER_SEAT_QUEUE as u64, 0));
    assert!(!hub.publish(
        "seat",
        SignalOut::Answer {
            sdp: "overflow".into()
        }
    ));
    assert_eq!(
        hub.counts(),
        (PER_SEAT_QUEUE as u64, 1),
        "a full queue drops and counts"
    );
    assert_eq!(
        replies.rx.blocking_recv().unwrap(),
        SignalOut::Answer { sdp: "0".into() }
    );
    assert!(hub.publish(
        "seat",
        SignalOut::Answer {
            sdp: "again".into()
        }
    ));
    assert_eq!(hub.counts(), ((PER_SEAT_QUEUE + 1) as u64, 1));
}

#[test]
fn attach_replaces_the_route_and_detach_respects_generation() {
    let hub = SignalHub::new("[]".into());
    let first = hub.attach("seat");
    let _second = hub.attach("seat");
    assert_eq!(hub.peers(), 1, "one route per seat");
    drop(first);
    assert_eq!(hub.peers(), 1);
    assert!(hub.publish("seat", SignalOut::Answer { sdp: "v=0".into() }));
    assert_eq!(hub.counts(), (1, 0));
    drop(_second);
    assert_eq!(hub.peers(), 0);
    assert!(!hub.publish("seat", SignalOut::Answer { sdp: "v=0".into() }));
    assert_eq!(hub.counts(), (1, 1));
}

#[test]
fn the_inbound_queue_is_bounded_and_post_never_blocks() {
    let hub = SignalHub::new("[]".into());
    let _rx = hub.take_signals().expect("first hand-over succeeds");
    assert!(
        hub.take_signals().is_none(),
        "the inbound queue is handed over exactly once"
    );
    let seat = Seat {
        id: "s".into(),
        room: "r".into(),
        token: "t".into(),
        expires_at: u64::MAX,
    };
    for _ in 0..INBOUND_QUEUE {
        assert!(hub.post(SignalIn::Offer {
            seat: seat.clone(),
            sdp: "v=0".into()
        }));
    }
    assert!(!hub.post(SignalIn::Offer {
        seat,
        sdp: "one too many".into()
    }));
}
