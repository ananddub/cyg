use super::*;
use engine_bus::RoomSnapshot;
use engine_physics::PhysicsWorld3D;

/// The guest capabilities these tests need. Storage is deny-by-default (config.toml has to
/// ask for it), so a persistence test that leaves the default in place can never save — and
/// `drain_rooms` then has nothing to send.
fn storage_perms() -> engine_wasm::AbiPermissions {
    engine_wasm::AbiPermissions {
        storage: true,
        ..Default::default()
    }
}

#[test]
fn drain_saves_the_state_of_every_room_it_closes() {
    let mut server = GameServer::new(None, 16, PhysicsWorld3D::default(), storage_perms());
    server.get_or_create_ecs("arena", None);

    let mut t = transport();
    let bus = Bus::new(8, 8);
    let mut cfg = engine_config::GameConfig::default();
    cfg.permissions.storage = true;
    drain_rooms(&mut server, &mut t, &bus, &cfg, "tank-3d");

    let saved = bus
        .snapshots_rx
        .try_recv()
        .expect("a simulated room's state rode the bus");
    assert_eq!(saved.room, "arena");
    assert_eq!(saved.backend, "ecs");
    assert_eq!(saved.seq, 1, "the room's first save is numbered one");
    assert!(
        server.has_snapshot("arena"),
        "the host kept its own copy for the room's next life"
    );
    let match_ = bus.matches_rx.try_recv().expect("the match is queued");
    assert_eq!(
        match_.snapshot_seq, 1,
        "and the match row points at that save"
    );
}

#[test]
fn a_cold_start_asks_the_store_for_its_persistent_rooms() {
    let mut cfg = engine_config::GameConfig::default();
    cfg.name = "arena".into();
    cfg.lifecycle.persistent_rooms = vec!["lobby".into()];
    // Without storage permission `rehydrate_persistent_rooms` returns before asking for
    // anything, so the pump below would wait forever on a fetch that is never sent.
    cfg.permissions.storage = true;
    let mut server = GameServer::new(None, 16, PhysicsWorld3D::default(), storage_perms());
    let bus = Bus::new(8, 8);

    let reader = bus.clone();
    let pump = std::thread::spawn(move || {
        for _ in 0..2 {
            // A bounded wait: a missing fetch must fail this test, not hang the suite.
            let fetch = reader
                .fetches_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("a fetch arrives");
            let saved = match fetch.room.as_str() {
                "lobby" => Some(RoomSnapshot {
                    room: "lobby".into(),
                    backend: "wasm".into(),
                    seq: 3,
                    bytes: vec![0xab, 0xcd],
                }),

                _ => None,
            };
            let _ = fetch.reply.send(saved);
        }
    });

    rehydrate_persistent_rooms(&mut server, &bus, &cfg, Duration::from_secs(5));
    pump.join().unwrap();

    assert_eq!(server.load_snapshot("lobby"), Some(&vec![0xab, 0xcd]));
    assert!(
        !server.has_snapshot("arena"),
        "a room with nothing stored starts empty, as it should"
    );
}

#[test]
fn a_cold_start_without_anything_to_restore_does_not_wait_on_it() {
    let mut cfg = engine_config::GameConfig::default();
    cfg.name = "arena".into();
    cfg.lifecycle.persistent_rooms = vec!["lobby".into()];
    cfg.permissions.storage = false;
    let mut server = GameServer::new(None, 16, PhysicsWorld3D::default(), cfg.permissions);
    let bus = Bus::new(8, 8);

    rehydrate_persistent_rooms(&mut server, &bus, &cfg, Duration::from_secs(5));
    assert!(
        bus.fetches_rx.try_recv().is_err(),
        "a game that cannot save is not asked for its saves"
    );

    cfg.permissions.storage = true;
    let started = Instant::now();
    rehydrate_persistent_rooms(&mut server, &bus, &cfg, Duration::from_millis(20));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "an unanswered fetch cannot hang a boot"
    );
    assert!(!server.has_snapshot("lobby"));
}

#[test]
fn a_drain_gets_its_grace_window_before_the_loop_stops() {
    let grace = Duration::from_secs(15);
    let mut drain = engine_manager::DrainState::new(grace);
    let started = Instant::now();

    assert!(
        !drain.expired(started),
        "no request, so nothing to wait for"
    );
    assert!(drain.begin(started), "the first pass opens the window");
    assert!(!drain.begin(started), "a later pass does not reopen it");
    assert!(
        !drain.expired(started + Duration::from_secs(14)),
        "the rooms are still inside their grace"
    );
    assert!(
        drain.expired(started + grace),
        "at the deadline the loop may finish and leave"
    );
}
