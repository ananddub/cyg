use super::*;
use crate::rooms::GameRoom;

#[test]
fn ecs_room_movement_and_delta_flow() {
    let mut room = GameRoom::new_ecs("ecs-arena", None);
    assert!(room.is_ecs());
    let id = room.join("sess_1", 0.0, 0.0);
    assert_eq!(id, 1);

    room.ecs_input("sess_1", &[input_flags::D]);
    let mut moved = 0.0f32;
    let mut saw_delta = false;
    for _ in 0..30 {
        room.tick_ecs(1.0 / 60.0);
        for msg in room.drain_outbox() {
            assert_eq!(msg.event, "delta");
            assert_eq!(msg.target, 0, "delta broadcasts room-wide");
            saw_delta = true;
            let mut i = 0;
            while i + 26 <= msg.payload.len() {
                assert_eq!(msg.payload[i], 0x01);
                assert_eq!(msg.payload[i + 1], 24);
                let eid = u64::from_le_bytes(msg.payload[i + 2..i + 10].try_into().unwrap());
                let x = f32::from_le_bytes(msg.payload[i + 10..i + 14].try_into().unwrap());
                if eid == 1 {
                    moved = x;
                }
                i += 26;
            }
        }
    }
    assert!(saw_delta, "delta must flow through the outbox");
    assert!(
        moved > 20.0,
        "holding D for half a second must move the avatar +x, got {moved}"
    );
}

#[test]
fn ecs_room_leave_stops_delta() {
    let mut room = GameRoom::new_ecs("ecs-leave", None);
    room.join("sess_1", 0.0, 0.0);
    room.release_player("sess_1", 1);
    room.tick_ecs(1.0 / 60.0);
    room.drain_outbox();
    room.tick_ecs(1.0 / 60.0);
    let drained = room.drain_outbox();
    for msg in &drained {
        let payload = &msg.payload;
        assert!(
            payload.len() < 26,
            "despawned avatar must not appear in deltas"
        );
    }
    assert!(room.ecs.as_ref().unwrap().player_count() == 0);
}

#[test]
fn room_rng_seed_replay_is_exact() {
    let mut a = EcsRoomState::new("match-ranked-asia-49210", None);
    let mut b = EcsRoomState::new("match-ranked-asia-49210", None);
    let seq_a: Vec<f32> = (0..16).map(|_| a.rng().next_f32()).collect();
    let seq_b: Vec<f32> = (0..16).map(|_| b.rng().next_f32()).collect();
    assert_eq!(seq_a, seq_b);
    let mut c = EcsRoomState::new("match-ranked-asia-49211", None);
    let seq_c: Vec<f32> = (0..16).map(|_| c.rng().next_f32()).collect();
    assert_ne!(seq_a, seq_c);
}

#[test]
fn ecs_room_oxcl_arena_raycast() {
    let arena = engine_physics::oxcl::CookedMesh {
        vertices: vec![
            [-50.0, 0.0, -50.0],
            [50.0, 0.0, -50.0],
            [50.0, 0.0, 50.0],
            [-50.0, 0.0, 50.0],
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let mut room = EcsRoomState::new("ecs-ray", Some(&arena));
    let hit = room.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "ray down must hit the baked OXCL arena");
    let (_entity, dist) = hit.unwrap();
    assert!((dist - 10.0).abs() < 0.5, "hit at floor level, got {dist}");
}
