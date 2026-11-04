use super::*;

    #[test]
    fn the_operator_ceiling_clamps_the_guest_hold() {
        let mut room = lifecycle_room();
        let id = room.join("dave", 0.0, 0.0, b"");
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
        let id = room.join("erin", 0.0, 0.0, b"");
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
        let id = room.join("frank", 0.0, 0.0, b"");
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
        let id = room.join("carol", 0.0, 0.0, b"");
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
        let id = room.join("bob", 0.0, 0.0, b"");
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
        assert!(room.should_send_patch(t0, 0), "first window is always due");
        assert!(
            !room.should_send_patch(t0 + Duration::from_millis(50), 0),
            "inside the 100ms window the patch is skipped"
        );
        assert!(
            room.take_patch_skipped(),
            "the skipped window must force a full snapshot"
        );
        assert!(!room.take_patch_skipped(), "the flag is consumed once");
        assert!(room.should_send_patch(t0 + Duration::from_millis(101), 0));
    }

    #[test]
    fn patch_due_wiring_reaches_the_guest_and_marks_sends() {
        let mut room = lifecycle_room();
        assert!(!room.patch_due_wired, "unwired until the first signal");

        room.signal_patch_due(true);
        assert!(room.patch_due_wired, "the guest exports on_patch_due");
        assert_eq!(guest_i32(&mut room, "patch_due_state"), 1);

        room.signal_patch_due(false);
        assert_eq!(guest_i32(&mut room, "patch_due_state"), 0);

        // mark_patch_sent anchors the window and clears a pending skip.
        let t0 = Instant::now();
        room.patch_skipped = true;
        room.mark_patch_sent(t0);
        assert!(!room.patch_skipped);
        assert!(!room.next_patch_due(t0 + Duration::from_millis(50), 0));
        assert!(room.next_patch_due(t0 + Duration::from_millis(101), 0));
    }

    #[test]
    fn next_patch_due_is_pure_and_obeys_the_operator_cap() {
        let mut room = lifecycle_room(); // guest rate 100ms
        let t0 = Instant::now();
        // No send yet -> due regardless of the cap.
        assert!(room.next_patch_due(t0, 0));
        assert!(room.next_patch_due(t0, 500));
        room.mark_patch_sent(t0);
        // Inside the guest window: not due, and the call mutated nothing.
        assert!(!room.next_patch_due(t0 + Duration::from_millis(50), 0));
        assert!(!room.patch_due_wired || room.last_patch_at == Some(t0));
        // Operator cap of 200ms throttles the 100ms guest past 150ms.
        assert!(!room.next_patch_due(t0 + Duration::from_millis(150), 200));
        assert!(room.next_patch_due(t0 + Duration::from_millis(201), 200));
        // A faster cap never accelerates the guest: at t0+80 the 100ms guest
        // window still governs, cap of 20ms notwithstanding.
        assert!(!room.next_patch_due(t0 + Duration::from_millis(80), 20));
        assert!(room.next_patch_due(t0 + Duration::from_millis(101), 20));
    }

    #[test]
    fn operator_patch_rate_can_throttle_but_never_accelerate_the_guest() {
        let mut room = lifecycle_room(); // guest declares on_patch_rate = 100ms
        let t0 = Instant::now();
        // Cap of 200ms overrides the guest's 100ms: the 150ms mark is skipped.
        assert!(room.should_send_patch(t0, 200));
        assert!(
            !room.should_send_patch(t0 + Duration::from_millis(150), 200),
            "inside the 200ms operator window the patch is skipped"
        );
        assert!(room.should_send_patch(t0 + Duration::from_millis(201), 200));
        // A cap FASTER than the guest (20ms) must not accelerate: the guest's
        // own 100ms window (anchored at the t0+201 send) still governs.
        assert!(!room.should_send_patch(t0 + Duration::from_millis(210), 20));
        assert!(room.should_send_patch(t0 + Duration::from_millis(302), 20));
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
