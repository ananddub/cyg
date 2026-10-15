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
    assert_eq!(current().unwrap().seq, on_bus.seq);
    // Window rings reset after publishing.
    assert_eq!(r.n_input, 0);
    assert_eq!(r.n_tick, 0);
    // Sub-second calls are no-ops, and say so.
    assert!(!r.publish_due());
    assert!(!r.maybe_publish(&bus, publish_report()));
    assert!(bus.status_rx.try_recv().is_err());
}

/// The histogram is the bar chart's only source, so it must count every tick
/// of the window it describes and roll over with the rings.
#[test]
fn tick_histogram_accumulates_and_rolls_with_the_window() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    // 1 µs and 3 µs land in buckets 0 and 1; 5 ms is past the top bucket.
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
    // The next window starts empty, even before anyone publishes again.
    let fresh = r.snapshot();
    assert_eq!(fresh.tick_histogram, [0; TICK_HISTOGRAM_BUCKETS]);
    assert_eq!(fresh.tick_samples, 0);
}

/// `nominal_tick_us` is the "are we keeping up" ruler, and `empty_for_sec` is
/// the only proof an idle process is idle rather than unreachable.
#[test]
fn nominal_period_and_idle_age_come_from_the_report() {
    let bus = Bus::new(16, 16);
    let mut r = Recorder::new();
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, PublishReport { tick_rate_hz: 128, players: 1, ..Default::default() }));
    let on_bus = bus.status_rx.try_recv().unwrap();
    assert_eq!(on_bus.nominal_tick_us, 7_812);
    assert_eq!(on_bus.empty_for_sec, 0, "occupied rooms are never idle");

    r.last_publish = Instant::now() - Duration::from_secs(2);
    r.empty_since = Some(Instant::now() - Duration::from_secs(7));
    assert!(r.maybe_publish(&bus, PublishReport { tick_rate_hz: 64, ..Default::default() }));
    let idle = bus.status_rx.try_recv().unwrap();
    assert_eq!(idle.nominal_tick_us, 15_625);
    assert_eq!(idle.empty_for_sec, 7);

    // A loop that does not know its configured rate reports 0 rather than the
    // 128 Hz period it happens to default its overrun check to.
    r.last_publish = Instant::now() - Duration::from_secs(2);
    assert!(r.maybe_publish(&bus, PublishReport::default()));
    assert_eq!(bus.status_rx.try_recv().unwrap().nominal_tick_us, 0);
}
