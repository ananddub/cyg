use super::*;

    #[test]
    fn test_multi_room_autocreate_and_autoclose() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );

        let room_a = server.get_or_create("room_a").expect("relay room");
        room_a.join("p1_a", 0.0, 0.0);
        room_a.join("p2_a", 10.0, 10.0);
        assert!(!room_a.is_hibernating());

        let room_b = server.get_or_create("room_b").expect("relay room");
        room_b.join("p1_b", 0.0, 0.0);
        assert!(!room_b.is_hibernating());
        assert_eq!(server.rooms.len(), 2);

        let room_a = server.rooms.get_mut("room_a").unwrap();
        room_a.handle_disconnect("p1_a");
        assert!(!room_a.is_hibernating());
        room_a.handle_disconnect("p2_a");
        assert!(room_a.is_hibernating());
        assert!(room_a.empty_since.is_some());

        assert!(!server.rooms.get("room_b").unwrap().is_hibernating());

        server.rooms.get_mut("room_a").unwrap().empty_since =
            Some(Instant::now() - Duration::from_secs(31));

        let mut dead_rooms = Vec::new();
        for (name, room) in server.rooms.iter_mut() {
            if room.is_hibernating() && room.reap_check(Duration::from_secs(30)) {
                dead_rooms.push(name.clone());
            }
        }
        for name in dead_rooms {
            server.rooms.remove(&name);
        }

        assert_eq!(server.rooms.len(), 1);
        assert!(!server.rooms.contains_key("room_a"));
        assert!(server.rooms.contains_key("room_b"));

        let room_a_fresh = server.get_or_create("room_a").expect("relay room");
        room_a_fresh.join("p3_a", 0.0, 0.0);
        assert!(!room_a_fresh.is_hibernating());
        assert_eq!(server.rooms.len(), 2);
    }

    #[test]
    fn test_room_persistence_autosave_and_autoload() {
        let mut server = GameServer::new(
            None,
            16,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );

        let room = server
            .get_or_create("world_zone_survival")
            .expect("relay room");
        room.join("player_builder", 0.0, 0.0);

        room.handle_disconnect("player_builder");
        assert!(room.is_hibernating());

        let mut room = server.rooms.remove("world_zone_survival").unwrap();
        assert!(
            room.export_snapshot().is_none(),
            "relay rooms have no state"
        );
        server.rooms.insert("world_zone_survival".into(), room);

        let snap = serde_json::to_vec(&RoomSnapshotData::Wasm(vec![1, 2, 3, 4])).expect("snapshot");
        server.save_snapshot("world_zone_survival", snap);
        assert!(server.has_snapshot("world_zone_survival"));

        let has_snapshot = server.has_snapshot("world_zone_survival");
        let restored_wasm = {
            let restored = server
                .get_or_create("world_zone_survival")
                .expect("restored room");
            restored.wasm.is_none()
        };
        assert_eq!(server.rooms.len(), 1);
        assert!(has_snapshot);
        assert!(restored_wasm);
    }
