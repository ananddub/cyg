use super::*;

#[test]
fn test_relay_room_lifecycle() {
    let mut room = GameRoom::new_relay();
    assert!(room.is_relay());
    assert!(!room.is_wasm());
    assert!(room.wasm.is_none());
    assert!(room.is_hibernating());

    let res = room.tick(0.016, false);
    assert_eq!(res.unwrap(), 0);

    let p1 = room.join("sess_alice", 50.0, 50.0, b"");
    assert_eq!(p1, 1);
    assert!(!room.is_hibernating());
    assert!(room.empty_since.is_none());

    room.make_spectator("sess_alice");
    assert_eq!(room.sessions.len(), 0);
    assert_eq!(room.spectators.len(), 1);
    assert!(!room.is_hibernating());

    room.handle_disconnect("sess_alice");
    assert!(room.is_hibernating());
    assert!(room.empty_since.is_some());
}

#[test]
fn test_room_hibernation_and_reap_timeout() {
    let mut room = GameRoom::new_relay();
    assert!(room.is_hibernating());

    assert!(!room.reap_check(Duration::from_secs(30)));

    room.empty_since = Some(Instant::now() - Duration::from_secs(31));
    assert!(room.reap_check(Duration::from_secs(30)));

    let _ = room.join("sess_bob", 10.0, 10.0, b"");
    assert!(!room.is_hibernating());
    assert!(!room.reap_check(Duration::from_secs(30)));
}
