use crate::rapier::*;
use rapier2d::prelude as r2d;
use rapier3d::prelude as r3d;

#[test]
fn test_rapier3d_gravity_fall_and_ground_rest() {
    let mut world = RapierWorld3D::default();

    let ground_collider = r3d::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    world.collider_set.insert(ground_collider);

    let sphere_rb = r3d::RigidBodyBuilder::dynamic()
        .translation(r3d::Vector::new(0.0, 10.0, 0.0))
        .build();
    let sphere_handle = world.rigid_body_set.insert(sphere_rb);
    let sphere_collider = r3d::ColliderBuilder::ball(0.5).build();
    world.collider_set.insert_with_parent(
        sphere_collider,
        sphere_handle,
        &mut world.rigid_body_set,
    );

    for _ in 0..120 {
        world.step();
    }

    let body = &world.rigid_body_set[sphere_handle];
    let final_y = body.translation().y;

    assert!(final_y < 1.0, "Ball must have reached ground plane");
    assert!(final_y >= 0.40, "Ball must not fall through ground plane");
}

#[test]
fn test_rapier2d_simulation() {
    let mut world = RapierWorld2D::default();

    let floor = r2d::ColliderBuilder::cuboid(50.0, 1.0)
        .translation(r2d::Vector::new(0.0, -1.0))
        .build();
    world.collider_set.insert(floor);

    let box_rb = r2d::RigidBodyBuilder::dynamic()
        .translation(r2d::Vector::new(0.0, 5.0))
        .build();
    let handle = world.rigid_body_set.insert(box_rb);
    let box_col = r2d::ColliderBuilder::cuboid(0.5, 0.5).build();
    world
        .collider_set
        .insert_with_parent(box_col, handle, &mut world.rigid_body_set);

    for _ in 0..120 {
        world.step();
    }

    let y = world.rigid_body_set[handle].translation().y;
    assert!(y < 1.0);
    assert!(y >= 0.40);
}

#[test]
fn test_rapier2d_check_intersection() {
    let mut world = RapierWorld2D::new(0.0, 0.0);
    let coin = world.create_body(1, 400.0, 250.0);
    world.add_circle_collider(coin, 24.0, true, 0.0, 0.0);

    let player = world.create_body(0, 400.0, 250.0);
    world.add_circle_collider(player, 14.0, false, 0.0, 0.0);

    world.step();

    let hit = world.check_intersection(player, coin);
    tracing::info!("check_intersection result: {hit}");
    assert!(hit, "Player and coin at same position must intersect!");

    world.set_position(coin, 100.0, 100.0);
    world.step();
    let hit_far = world.check_intersection(player, coin);
    tracing::info!("after move coin far: {hit_far}");
    assert!(!hit_far, "Should not intersect when far");

    world.set_position(player, 100.0, 100.0);
    world.step();
    let hit_moved = world.check_intersection(player, coin);
    tracing::info!("after move player to coin: {hit_moved}");
    assert!(
        hit_moved,
        "Should intersect after moving both to (100, 100)"
    );
}
