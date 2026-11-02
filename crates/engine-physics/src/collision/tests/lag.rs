use crate::collision::*;

#[test]
fn test_find_snapshot_binary_search_matches_linear_scan() {
    let mut comp = LagCompensator::new(64);
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut tick = 0u64;
    for i in 0..200u64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        tick += 1 + seed % 3;
        comp.record_entity(
            tick,
            i,
            HistoricalEntity {
                entity_id: i as u32,
                kind: 1,
                bounds: Aabb {
                    min: Vec3::ZERO,
                    max: Vec3::ZERO,
                },
            },
        );
    }

    let linear = |shoot_tick: u64| -> Option<u64> {
        if let Some(s) = comp.history.iter().find(|s| s.tick == shoot_tick) {
            return Some(s.tick);
        }
        let oldest = comp.history.front().unwrap().tick;
        if shoot_tick < oldest.saturating_sub(60) {
            return None;
        }
        comp.history
            .iter()
            .min_by_key(|s| (s.tick as i64 - shoot_tick as i64).abs())
            .map(|s| s.tick)
    };

    for t in tick.saturating_sub(140)..=tick + 40 {
        assert_eq!(
            comp.find_snapshot(t).map(|s| s.tick),
            linear(t),
            "shoot_tick={t}"
        );
    }
}

#[test]
fn test_lag_compensation_rewind_success() {
    let mut world = PhysicsWorld3D::new(400.0, 400.0);

    let box_tick_100 = Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
    world.record_entity_hitbox(100, 1000, 42, 1, box_tick_100);

    let box_tick_105 = Aabb::from_center_size(Vec3::new(0.0, 5.0, 80.0), Vec3::new(4.0, 4.0, 4.0));
    world.record_entity_hitbox(105, 1083, 42, 1, box_tick_105);

    let ray_origin = Vec3::new(0.0, 5.0, 0.0);
    let ray_dir = Vec3::new(0.0, 0.0, 1.0);

    let hit_100 = world.lag_compensated_raycast(100, ray_origin, ray_dir, 100.0);
    assert!(hit_100.is_some(), "Rewind to tick 100 must hit target");
    let h100 = hit_100.unwrap();
    assert_eq!(h100.entity_id, 42);
    assert_eq!(h100.hit_type, 1);
    assert!((h100.distance - 38.0).abs() < 1e-4);

    let hit_105 = world.lag_compensated_raycast(105, ray_origin, ray_dir, 50.0);
    assert!(hit_105.is_none(), "Target moved out of range at tick 105");
}

#[test]
fn test_lag_compensation_wall_occlusion() {
    let mut world = PhysicsWorld3D::new(400.0, 400.0);

    world.add_box_obstacle(
        99,
        "concrete_wall",
        Vec3::new(0.0, 5.0, 20.0),
        Vec3::new(20.0, 10.0, 2.0),
        false,
    );

    let target_box = Aabb::from_center_size(Vec3::new(0.0, 5.0, 40.0), Vec3::new(4.0, 4.0, 4.0));
    world.record_entity_hitbox(50, 500, 77, 1, target_box);

    let ray_origin = Vec3::new(0.0, 5.0, 0.0);
    let ray_dir = Vec3::new(0.0, 0.0, 1.0);

    let hit = world.lag_compensated_raycast(50, ray_origin, ray_dir, 100.0);
    assert!(hit.is_some());
    let h = hit.unwrap();

    assert_eq!(h.hit_type, 2, "Wall must occlude line of sight");
    assert_eq!(h.collider_id, 99);
    assert_eq!(h.entity_id, 0);
}
