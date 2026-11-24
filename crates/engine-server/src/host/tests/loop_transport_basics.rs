use super::*;

#[test]
fn reject_room_on_an_unknown_session_is_a_noop() {
    let mut t = transport();
    reject_room(&mut t, "sess_missing", "room is full");

    let mut events = Vec::new();
    t.poll_events(Duration::ZERO, &mut events).unwrap();
    assert!(events.is_empty(), "unexpected events: {events:?}");
    assert_eq!(t.ccu(), 0);
}

#[test]
fn local_addr_reports_the_bound_socket() {
    let t = transport();
    assert_ne!(t.local_addr().unwrap().port(), 0);
}

#[test]
fn latency_queues_outbound_until_the_one_way_delay_elapses() {
    let mut t = transport();
    t.set_latency(Duration::from_millis(40));
    t.send_to("ghost", OpCode::Custom, b"payload").unwrap();
    assert_eq!(t.delayed_out.len(), 1, "the send is held, not delivered");

    let mut events = Vec::new();
    t.poll_events(Duration::ZERO, &mut events).unwrap();
    assert_eq!(
        t.delayed_out.len(),
        1,
        "before the one-way delay nothing flushes"
    );

    std::thread::sleep(Duration::from_millis(50));
    t.poll_events(Duration::ZERO, &mut events).unwrap();
    assert_eq!(
        t.delayed_out.len(),
        0,
        "after the delay the message is released to the wire"
    );
}

#[test]
fn zero_latency_keeps_no_queues_at_all() {
    let mut t = transport();
    t.send_to("ghost", OpCode::Custom, b"payload").unwrap();
    assert!(
        t.delayed_out.is_empty(),
        "no latency = no queue, direct send"
    );
    assert!(t.delayed_in.is_empty());
}

#[test]
fn room_name_sanitizer_drops_query_and_session_suffix() {
    assert_eq!(sanitize_room_name("smoke"), "smoke");
    assert_eq!(sanitize_room_name("smoke?sessionId=alice&token=x"), "smoke");
    assert_eq!(sanitize_room_name("smoke?room=ignored"), "smoke");
    assert_eq!(sanitize_room_name("smoke/sessionId=alice"), "smoke/");
}
