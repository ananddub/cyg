use super::*;

#[test]
fn server_without_room_module_serves_relay_rooms() {
    let mut server = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions::default(),
    );
    assert!(!server.has_room_module());

    let room = server.get_or_create("plain_room", b"").expect("relay room");
    assert!(room.is_relay());
    assert!(room.wasm.is_none());

    let _ = server.get_or_create("plain_room", b"");
    assert_eq!(server.rooms.len(), 1);
}

#[test]
fn room_limit_blocks_only_new_rooms() {
    let mut server = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions::default(),
    );
    let _ = server.get_or_create("a", b"");

    assert!(server.would_exceed_room_limit("b", 1));
    assert!(!server.would_exceed_room_limit("a", 1));
    assert!(!server.would_exceed_room_limit("b", 0));
    assert!(!server.would_exceed_room_limit("b", 2));
}

#[test]
fn relay_room_reports_capacity_at_the_ceiling() {
    let mut room = GameRoom::new_relay();
    assert!(!room.is_at_capacity(2));
    room.join("a", 0.0, 0.0, b"");
    assert!(!room.is_at_capacity(2), "one of two is not full");
    room.join("b", 0.0, 0.0, b"");
    assert!(room.is_at_capacity(2), "at the ceiling");
    assert!(!room.is_at_capacity(0), "0 means unlimited");
    assert!(!room.is_at_capacity(-1), "negative means unlimited");

    // Variant overrides ([rooms.x] max_clients) apply to every backend —
    // a wasm room reports capacity exactly like a relay room.
    let mut wasm_room = GameRoom::new_relay();
    wasm_room.backend = RoomBackend::Wasm;
    wasm_room.join("c", 0.0, 0.0, b"");
    assert!(!wasm_room.is_at_capacity(2), "one of two is not full");
    wasm_room.join("d", 0.0, 0.0, b"");
    assert!(
        wasm_room.is_at_capacity(2),
        "variant cap binds wasm rooms too"
    );
}
