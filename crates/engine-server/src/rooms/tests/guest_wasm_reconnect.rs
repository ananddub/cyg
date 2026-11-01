use super::*;

    #[test]
    fn the_operator_ceiling_clamps_the_guest_hold() {
        let mut room = lifecycle_room();
        let id = room.join("dave", 0.0, 0.0);
        assert_eq!(
            room.handle_unexpected_disconnect("dave", Duration::from_secs(2)),
            None
        );
        let deadline = room.pending_deadline("dave").expect("seat held");
        assert!(
            deadline <= Instant::now() + Duration::from_millis(2100),
            "hold must clamp to the operator ceiling, got {deadline:?}"
        );
        assert_eq!(room.sessions.get("dave"), Some(&id));
    }

    #[test]
    fn a_zero_hold_releases_the_seat_immediately() {
        let mut room = lifecycle_room();
        let id = room.join("erin", 0.0, 0.0);
        assert_eq!(
            room.handle_unexpected_disconnect("erin", Duration::from_secs(0)),
            None
        );
        assert!(
            !room.has_pending_leave("erin"),
            "a zero ceiling must not hold the seat"
        );
        assert!(
            !room.sessions.contains_key("erin"),
            "the seat is released right away"
        );
        assert_eq!(guest_i32(&mut room, "leave_count"), 1);
        let _ = id;
    }

    #[test]
    fn takeover_keeps_the_seat_and_fires_on_reconnect() {
        let mut room = lifecycle_room();
        let id = room.join("frank", 0.0, 0.0);
        assert_eq!(room.takeover("frank"), Some(id));
        assert_eq!(room.sessions.get("frank"), Some(&id), "seat preserved");
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 1);
        assert_eq!(guest_i32(&mut room, "leave_count"), 0, "no onLeave for a takeover");
        assert_eq!(room.takeover("ghost"), None);
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 1);
    }

    #[test]
    fn consented_disconnect_releases_a_held_seat_immediately() {
        let mut room = lifecycle_room();
        let id = room.join("carol", 0.0, 0.0);
        room.handle_unexpected_disconnect("carol", Duration::from_secs(120));
        assert!(room.has_pending_leave("carol"));

        assert_eq!(room.rejoin("carol"), Some(id));
        assert_eq!(room.handle_disconnect("carol"), None);
        assert!(!room.has_pending_leave("carol"));
        assert!(room.sessions.is_empty());
        assert_eq!(guest_i32(&mut room, "leave_count"), 1);
        assert!(room.is_hibernating());
    }

    #[test]
    fn reconnect_after_the_deadline_is_a_fresh_join() {
        let mut room = lifecycle_room();
        let id = room.join("bob", 0.0, 0.0);
        room.handle_unexpected_disconnect("bob", Duration::from_secs(120));
        room.pending_leaves
            .insert("bob".to_string(), (id, Instant::now()));
        assert!(room.expire_pending_leaves(Instant::now()));
        assert_eq!(room.rejoin("bob"), None);
        assert_eq!(guest_i32(&mut room, "reconnect_count"), 0);
    }

    #[test]
    fn patch_rate_skips_windows_and_forces_the_next_full() {
        let mut room = lifecycle_room();
        let t0 = Instant::now();
        assert!(room.should_send_patch(t0), "first window is always due");
        assert!(
            !room.should_send_patch(t0 + Duration::from_millis(50)),
            "inside the 100ms window the patch is skipped"
        );
        assert!(
            room.take_patch_skipped(),
            "the skipped window must force a full snapshot"
        );
        assert!(!room.take_patch_skipped(), "the flag is consumed once");
        assert!(room.should_send_patch(t0 + Duration::from_millis(101)));
    }

    #[test]
    fn simulation_rate_runs_at_a_fixed_dt_and_skips_intermediate_ticks() {
        let mut room = lifecycle_room();
        for _ in 0..4 {
            assert_eq!(room.tick(0.01, false).unwrap(), 0);
        }
        assert_eq!(
            guest_i32(&mut room, "tick_count"),
            0,
            "50ms cadence with 10ms host ticks: four ticks are not enough"
        );
        assert_eq!(room.tick(0.01, false).unwrap(), 0);
        assert_eq!(
            guest_i32(&mut room, "tick_count"),
            1,
            "the fifth 10ms tick reaches the guest"
        );
        assert_eq!(room.sim_accum, Duration::ZERO);
    }

    #[test]
    fn dispose_runs_the_guest_hook() {
        let mut room = lifecycle_room();
        room.dispose();
        assert_eq!(guest_i32(&mut room, "dispose_count"), 1);
    }
