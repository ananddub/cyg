use super::*;

    #[test]
    fn on_auth_sees_the_client_options_and_can_reject() {
        let mut room = lifecycle_room();
        assert!(room.authorize(1, r#"{"token":"ok"}"#));
        assert!(!room.authorize(2, "denied"));

        let mut relay = GameRoom::new_relay();
        assert!(relay.authorize(1, "anything"));
    }

    #[test]
    fn allow_reconnection_holds_the_seat_then_reconnect_resumes_it() {
        let mut room = lifecycle_room();
        let id = room.join("alice", 0.0, 0.0);
        assert_eq!(
            room.handle_unexpected_disconnect("alice", Duration::from_secs(120)),
            None
        );

        assert!(
            room.has_pending_leave("alice"),
            "a 5s hold must keep the seat"
        );
        assert_eq!(
            room.sessions.get("alice"),
            Some(&id),
            "the player id and guest slot survive the disconnect"
        );
        assert_eq!(guest_i32(&mut room, "leave_count"), 0);

        assert_eq!(room.rejoin("alice"), Some(id));
        assert!(!room.has_pending_leave("alice"));
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 1);
        assert_eq!(guest_i32(&mut room, "leave_count"), 0);

        assert_eq!(
            room.handle_unexpected_disconnect("alice", Duration::from_secs(120)),
            None
        );
        room.pending_leaves
            .insert("alice".to_string(), (id, Instant::now()));
        assert!(room.expire_pending_leaves(Instant::now()));
        assert!(room.sessions.is_empty());
        assert_eq!(guest_i32(&mut room, "leave_count"), 1);
        assert!(room.is_hibernating());
    }
