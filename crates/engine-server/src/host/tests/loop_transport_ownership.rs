use super::*;
use engine_physics::PhysicsWorld3D;

#[test]
fn drain_closes_and_forgets_every_room() {
    let mut server = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions::default(),
    );
    server.get_or_create("arena", b"").expect("relay room");
    server.get_or_create("lobby", b"").expect("relay room");

    let mut t = transport();
    let bus = Bus::new(8, 8);
    drain_rooms(
        &mut server,
        &mut t,
        &bus,
        &engine_config::GameConfig::default(),
        "tank-3d",
    );
    assert!(server.rooms.is_empty(), "a drained room kept its slot");
    let match_ = bus
        .matches_rx
        .try_recv()
        .expect("a drained room is a finished match");
    assert_eq!(match_.end_reason, DRAIN_REASON);
    assert_eq!(match_.game_id, "tank-3d");

    let mut names: Vec<String> = Vec::new();
    while let Ok(row) = bus.ownership_rx.try_recv() {
        assert_eq!(row.state, engine_bus::RoomPhase::Closed);
        assert_eq!(row.node, engine_config::node_id());
        names.push(row.room);
    }
    names.sort();
    assert_eq!(
        names,
        vec!["arena".to_string(), "lobby".to_string()],
        "every drained room's name came back to the registry"
    );
}

#[test]
fn a_beat_claims_the_room_for_this_node() {
    let mut server = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions::default(),
    );
    server.get_or_create("arena", b"").expect("relay room");
    let bus = Bus::new(8, 8);

    let room = server.rooms.get("arena").expect("the room is live");
    report_ownership(&bus, "tank-3d-7f9c", "tank-3d", "arena", room, room.phase());

    let row = bus.ownership_rx.try_recv().expect("the claim is queued");
    assert_eq!(row.room, "arena");
    assert_eq!(row.node, "tank-3d-7f9c");
    assert_eq!(row.game, "tank-3d");
    assert_eq!(row.state, engine_bus::RoomPhase::Created);
    assert_eq!(row.peers, 0);
    assert_eq!(
        row.snapshot_seq, 0,
        "a room that never saved has no ordinal"
    );
}

#[test]
fn a_saved_room_hands_its_bytes_and_backend_to_the_control_plane() {
    let bus = Bus::new(8, 8);
    let mut room = GameRoom::new_relay();
    room.snapshot_seq = 7;
    report_snapshot(&bus, &room, "arena", &[1, 2, 3]);
    let saved = bus.snapshots_rx.try_recv().expect("snapshot queued");
    assert_eq!(saved.room, "arena");
    assert_eq!(saved.backend, "relay");
    assert_eq!(saved.seq, 7);
    assert_eq!(saved.bytes, vec![1, 2, 3]);
    assert_eq!(backend_label(RoomBackend::Wasm), "wasm");
    assert_eq!(backend_label(RoomBackend::Ecs), "ecs");
}

#[test]
fn a_flooded_session_loses_only_its_own_surplus() {
    let tick = Duration::from_secs_f64(1.0 / 30.0);
    let mut rates: HashMap<std::rc::Rc<str>, netcode::InputRateLimiter> = HashMap::new();

    let flood = (0..1_000)
        .filter(|_| input_budget_allows(&mut rates, tick, "sess_flood"))
        .count();
    assert!(
        flood < 200,
        "a 1000-sample burst at 30 Hz should cost the flooder its surplus, got {flood}"
    );
    assert!(flood > 0, "the flooder's first samples must still simulate");

    for _ in 0..30 {
        assert!(
            input_budget_allows(&mut rates, tick, "sess_quiet"),
            "a second seat lost budget to the first one's flood"
        );
    }
}

#[test]
fn the_udp_lane_gates_budget_and_sample_shape() {
    let tick = Duration::from_secs_f64(1.0 / 30.0);
    let peer: SocketAddr = "127.0.0.1:9000".parse().unwrap();
    let mut rates: HashMap<SocketAddr, netcode::InputRateLimiter> = HashMap::new();

    let domains = netcode::standard_input_domains();
    let mut clean = [0u8; 20];
    clean[12..16].copy_from_slice(&0.5f32.to_le_bytes());
    assert!(udp_input_allowed(
        &mut rates, tick, peer, &mut clean, &domains
    ));

    let mut nan = clean;
    nan[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(
        udp_input_allowed(&mut rates, tick, peer, &mut nan, &domains),
        "sanitize clamps instead of dropping the budgeted frame"
    );
    assert_eq!(
        f32::from_le_bytes(nan[4..8].try_into().unwrap()),
        -std::f32::consts::PI,
        "a NaN aim lane reached the guest"
    );
}
