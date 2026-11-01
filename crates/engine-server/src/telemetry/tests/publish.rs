use super::*;

#[test]
fn publish_flows_to_bus_and_current() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.note_tick(Duration::from_micros(500));
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, publish_report()));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.players, 3);
    assert_eq!(on_bus.tick_budget_p50_us, 500);
    assert!(current().is_some());

    assert_eq!(r.n_input, 0);
    assert_eq!(r.n_tick, 0);

    assert!(!r.publish_due());
    assert!(!r.maybe_publish(&bus, publish_report()));
    assert!(bus.status_rx.try_recv().is_err());
}

#[test]
fn phase_aggregates_are_counted_from_the_rows() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.last_publish = Instant::now() - Duration::from_secs(2);
    let row = |name: &str, phase: engine_bus::RoomPhase| engine_bus::RoomTelemetry {
        name: name.into(),
        phase,
        ..Default::default()
    };
    assert!(r.maybe_publish(
        &bus,
        PublishReport {
            rooms: vec![
                row("lobby", engine_bus::RoomPhase::Active),
                row("arena", engine_bus::RoomPhase::Hibernating),
                row("spare", engine_bus::RoomPhase::Hibernating),
                row("fresh", engine_bus::RoomPhase::Created),
                row("gone", engine_bus::RoomPhase::Closed),
            ],
            ..Default::default()
        }
    ));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.rooms_active, 1);
    assert_eq!(on_bus.rooms_hibernating, 2);
    assert_eq!(on_bus.rooms_created, 1);

    assert_eq!(on_bus.rooms.len(), 5);
    assert_eq!(
        on_bus.rooms_created + on_bus.rooms_active + on_bus.rooms_hibernating,
        4
    );
}

#[test]
fn tick_histogram_accumulates_and_rolls_with_the_window() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();

    r.note_tick(Duration::from_micros(1));
    r.note_tick(Duration::from_micros(3));
    r.note_tick(Duration::from_millis(5));
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, publish_report()));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.tick_samples, 3);
    assert_eq!(on_bus.tick_histogram[0], 1);
    assert_eq!(on_bus.tick_histogram[1], 1);
    assert_eq!(on_bus.tick_histogram[TICK_HISTOGRAM_BUCKETS - 1], 1);
    assert_eq!(on_bus.tick_histogram.iter().sum::<u32>(), 3);

    let fresh = r.snapshot();
    assert_eq!(fresh.tick_histogram, [0; TICK_HISTOGRAM_BUCKETS]);
    assert_eq!(fresh.tick_samples, 0);
}

#[test]
fn nominal_period_and_idle_age_come_from_the_report() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(
        &bus,
        PublishReport {
            tick_rate_hz: 128,
            players: 1,
            ..Default::default()
        }
    ));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.nominal_tick_us, 7_812);
    assert_eq!(on_bus.empty_for_sec, 0, "occupied rooms are never idle");

    r.last_publish = Instant::now() - Duration::from_secs(2);
    r.empty_since = Some(Instant::now() - Duration::from_secs(7));
    assert!(r.maybe_publish(
        &bus,
        PublishReport {
            tick_rate_hz: 64,
            ..Default::default()
        }
    ));
    let idle = bus.status_rx.try_recv().unwrap();
    assert_eq!(idle.nominal_tick_us, 15_625);
    assert_eq!(idle.empty_for_sec, 7);

    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, PublishReport::default()));
    assert_eq!(bus.status_rx.try_recv().unwrap().nominal_tick_us, 0);
}
