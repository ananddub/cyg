use crate::collision::*;

#[test]
fn test_raycast_hit() {
    let mut world = PhysicsWorld3D::new(400.0, 400.0);
    world.add_box_obstacle(
        1,
        "bunker",
        Vec3::new(0.0, 10.0, 50.0),
        Vec3::new(20.0, 20.0, 20.0),
        true,
    );

    let hit = world.raycast(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 100.0);
    assert!(hit.is_some());
    let h = hit.unwrap();
    assert_eq!(h.collider_id, 1);
    assert!((h.distance - 40.0).abs() < 1e-4);
}

#[test]
fn test_broadphase_grid_matches_full_scan() {
    let mut world = PhysicsWorld3D::new(400.0, 400.0);
    let mut seed = 0x243F6A8885A308D3u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 800) as f64 - 400.0
    };
    for i in 0..60u32 {
        let size = Vec3::new(4.0 + (i % 7) as f64 * 8.0, 10.0, 4.0 + (i % 5) as f64 * 8.0);
        world.add_box_obstacle(i, "box", Vec3::new(rnd(), 5.0, rnd()), size, i % 3 == 0);
    }

    for _ in 0..400 {
        let (px, pz) = (rnd(), rnd());
        let radius = 2.0 + ((seed >> 13) % 40) as f64;
        let r = radius * 0.85;
        let py = 5.0;

        let mut expected_surface = 0.0;
        for obs in &world.obstacles {
            if obs.is_walkable_roof
                && px >= obs.bounds.min.x - r
                && px <= obs.bounds.max.x + r
                && pz >= obs.bounds.min.z - r
                && pz <= obs.bounds.max.z + r
                && py >= obs.height - 2.5
                && obs.height > expected_surface
            {
                expected_surface = obs.height;
            }
        }
        assert_eq!(
            world.get_surface_height(px, py, pz, radius),
            expected_surface
        );

        let dir = Vec3::new(rnd(), 0.0, rnd());
        let norm = dir.normalize();
        let origin = Vec3::new(px, py, pz);
        let max_dist = 120.0;
        let mut expected: Option<RayHit> = None;
        let mut nearest = max_dist;
        for obs in &world.obstacles {
            if let Some(mut hit) = obs.bounds.ray_intersect(origin, norm, nearest) {
                if hit.distance < nearest {
                    nearest = hit.distance;
                    hit.collider_id = obs.id;
                    expected = Some(hit);
                }
            }
        }
        assert_eq!(
            world.raycast(origin, dir, max_dist),
            expected,
            "raycast at {px},{pz}"
        );
    }
}

#[test]
fn test_move_and_slide() {
    let mut world = PhysicsWorld3D::new(400.0, 400.0);
    world.add_box_obstacle(
        1,
        "bunker",
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::new(20.0, 20.0, 20.0),
        true,
    );

    let res = world.move_and_slide(0.0, 0.0, -15.0, 0.0, 0.0, 10.0, 3.6, 1.0);
    assert!(res.is_walled);
    assert!(res.new_z <= -10.0 - 3.6 + 1e-4);
}
