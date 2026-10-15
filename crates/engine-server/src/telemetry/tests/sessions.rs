use super::*;

#[test]
fn a_publish_carries_the_session_rows_and_the_transport_mix() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.last_publish = Instant::now() - Duration::from_secs(2);
    let row = |session: &str, transport: &str| SessionTelemetry {
        session: session.into(),
        transport: transport.into(),
        player_id: Some(1),
        acked_seq: Some(42),
        idle_ms: Some(5),
    };
    assert!(r.maybe_publish(
        &bus,
        PublishReport {
            players: 2,
            ws_sessions: 1,
            udp_sessions: 1,
            sessions: vec![row("ws-1", "websocket"), row("[::1]:7778", "native-udp")],
            ..Default::default()
        }
    ));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.sessions.len(), 2);
    assert_eq!(on_bus.sessions[0].session, "ws-1");
    assert_eq!(on_bus.sessions[1].transport, "native-udp");
    assert_eq!(on_bus.sessions[0].acked_seq, Some(42));
    assert_eq!(on_bus.webrtc_sessions, 0);

    // The cap is a size guard on the payload, not a suggestion.
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(
        &bus,
        PublishReport { sessions: vec![row("a", "websocket"), row("b", "websocket")], session_cap: 1, ..Default::default() }
    ));
    assert_eq!(bus.status_rx.try_recv().unwrap().sessions.len(), 1);
}

/// A room that never ticked must publish a zeroed window rather than dividing
/// by an empty ring.
#[test]
fn publishing_an_empty_window_is_safe() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, PublishReport::default()));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.tick_samples, 0);
    assert_eq!(on_bus.tick_histogram, [0; TICK_HISTOGRAM_BUCKETS]);
    assert_eq!((on_bus.tick_budget_p50_us, on_bus.tick_budget_p99_us), (0, 0));
    assert!(on_bus.sessions.is_empty());
}
