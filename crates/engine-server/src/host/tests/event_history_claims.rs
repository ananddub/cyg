use super::*;
#[test]
fn an_unlisted_event_stays_on_the_wire() {
    let bus = Bus::new(8, 8);
    let record = engine_config::GameplayConfig {
        record_events: vec!["deal".into()],
        ..Default::default()
    };
    let history = EventHistory::new(&bus, &record);

    history.mirror("arena", 4, &message("chat"));
    assert!(bus.events_rx.try_recv().is_err(), "chat was never listed");

    history.mirror("arena", 5, &message("deal"));
    let row = bus.events_rx.try_recv().expect("deal was listed");
    assert_eq!(row.room, "arena");
    assert_eq!(row.tick, 5);
    assert_eq!(row.target, 7);
    assert_eq!(row.event.as_ref(), "deal");
    assert_eq!(row.seq, 1, "the ordinal counts what was actually queued");
}

#[test]
fn a_desync_is_recorded_whether_or_not_the_game_listed_anything() {
    let bus = Bus::new(8, 8);
    let record = engine_config::GameplayConfig::default();
    let history = EventHistory::new(&bus, &record);

    history.record_desync(
        "arena",
        3,
        &engine_wasm::DesyncReport {
            tick: 99,
            expected_checksum: 0x0123_4567_89ab_cdef,
            client_checksum: 0xfedc_ba98_7654_3210,
        },
    );

    let row = bus
        .events_rx
        .try_recv()
        .expect("the engine's own evidence is never filtered");
    assert_eq!(row.event.as_ref(), "desync");
    assert_eq!(row.room, "arena");
    assert_eq!(row.target, 3);
    assert_eq!(row.tick, 99);
    let expected = u64::from_le_bytes(row.payload[..8].try_into().unwrap());
    let client = u64::from_le_bytes(row.payload[8..].try_into().unwrap());
    assert_eq!(expected, 0x0123_4567_89ab_cdef);
    assert_eq!(client, 0xfedc_ba98_7654_3210);
}

#[test]
fn a_join_options_claim_bridges_the_seat_to_its_user() {
    let bus = Bus::new(8, 8);
    let record = engine_config::GameplayConfig::default();
    let history = EventHistory::new(&bus, &record);

    let user = "0195c8da-1111-7000-8000-000000000001";
    assert_eq!(
        claimed_user_id(&format!(r#"{{"userId":"{user}"}}"#)).as_deref(),
        Some(user)
    );
    assert!(bus.seats_rx.try_recv().is_err(), "nothing was bridged yet");

    assert_eq!(
        claimed_user_id(r#"{"userId":"someone"}"#).as_deref(),
        Some("someone")
    );

    history.bridge_seat("arena", 3, Some(user.to_string()));
    let seat = bus.seats_rx.try_recv().expect("the claim is queued");
    assert_eq!(seat.room, "arena");
    assert_eq!(seat.seat, 3);
    assert_eq!(seat.user_id.as_deref(), Some(user));
    assert!(
        bus.events_rx.try_recv().is_err(),
        "the bridge is not an event row"
    );

    history.bridge_seat("arena", 3, None);
    let seat = bus.seats_rx.try_recv().expect("the release is queued");
    assert_eq!(seat.user_id, None);
    assert_eq!(
        claimed_user_id("not json"),
        None,
        "unparseable options carry no claim"
    );
    assert_eq!(claimed_user_id(r#"{"userId":""}"#), None);
}

#[test]
fn stat_intents_are_coalesced_per_seat_and_counter() {
    let bus = Bus::new(8, 8);
    let record = engine_config::GameplayConfig::default();
    let history = EventHistory::new(&bus, &record);

    let stat: std::sync::Arc<str> = std::sync::Arc::from("kills");
    history.record_stats(
        "arena",
        9,
        vec![
            crate::rooms::StatIntent {
                seat: 3,
                stat: stat.clone(),
                delta: 1,
            },
            crate::rooms::StatIntent {
                seat: 3,
                stat: stat.clone(),
                delta: 2,
            },
            crate::rooms::StatIntent {
                seat: 3,
                stat: std::sync::Arc::<str>::from("deaths"),
                delta: 1,
            },
        ],
    );

    let first = bus.stats_rx.try_recv().expect("one coalesced write");
    let second = bus.stats_rx.try_recv().expect("the other counter");
    let (kills, deaths) = if first.stat == "kills" {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(kills.room, "arena");
    assert_eq!(kills.tick, 9);
    assert_eq!(kills.seat, 3);
    assert_eq!(kills.delta, 3);
    assert_eq!(deaths.stat, "deaths");
    assert_eq!(deaths.delta, 1);
    assert_ne!(kills.write_id, deaths.write_id);
    assert!(bus.stats_rx.try_recv().is_err());
}
