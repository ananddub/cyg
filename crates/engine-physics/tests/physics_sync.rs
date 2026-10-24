use engine_core::{EntityId, Position3D, TickRunner};
use engine_physics::{
    step_rapier_3d, sync_ecs_to_rapier_3d, sync_rapier_to_ecs_3d, RapierWorld3D,
};
use rapier3d::prelude as r3d;

#[test]
fn test_physics_ecs_sync_and_raycast() {
    let mut runner = TickRunner::new();

    // 1. Setup RapierWorld3D resource
    let mut rapier = RapierWorld3D::default();

    // Ground collider at y = 0
    let ground = r3d::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    rapier.collider_set.insert(ground);

    // Dynamic ball body for entity 42 at y = 10.0
    let ball_rb = r3d::RigidBodyBuilder::dynamic()
        .translation(r3d::Vector::new(0.0, 10.0, 0.0))
        .build();
    let ball_handle = rapier.rigid_body_set.insert(ball_rb);
    let ball_col = r3d::ColliderBuilder::ball(0.5).build();
    rapier
        .collider_set
        .insert_with_parent(ball_col, ball_handle, &mut rapier.rigid_body_set);

    // Bind entity 42 to this rigid body
    rapier.bind_entity(42, ball_handle);

    // Insert resource into Bevy World
    runner.world.insert_resource(rapier);

    // 2. Spawn ECS Entity with EntityId and Position3D
    let entity = runner.world.spawn((
        EntityId(42),
        Position3D {
            x: 0.0,
            y: 10.0,
            z: 0.0,
        },
    )).id();

    // 3. Add simulation systems in order: ECS -> Rapier -> Step -> Rapier -> ECS
    runner.add_system(sync_ecs_to_rapier_3d);
    runner.add_system(step_rapier_3d);
    runner.add_system(sync_rapier_to_ecs_3d);

    // 4. Tick simulation for 120 steps (2 seconds)
    for _ in 0..120 {
        runner.tick();
    }

    // 5. Verify ECS Position3D updated automatically from physics
    let pos = runner.world.entity(entity).get::<Position3D>().unwrap();
    println!("ECS Position3D after gravity ticks: {:?}", pos);
    assert!(
        pos.y < 1.0,
        "Ball position in ECS must drop to ground, got y={}",
        pos.y
    );
    assert!(
        pos.y >= 0.40,
        "Ball position in ECS must rest on ground, got y={}",
        pos.y
    );

    // 6. Test Raycast from above hits entity 42
    let mut rapier_res = runner.world.resource_mut::<RapierWorld3D>();
    let hit = rapier_res.raycast_hit([0.0, 20.0, 0.0], [0.0, -1.0, 0.0], 50.0);
    assert!(hit.is_some(), "Raycast should hit the ball");
    let (hit_id, dist) = hit.unwrap();
    assert_eq!(hit_id, 42, "Hit entity should be 42");
    // Ball rests at y ~ 0.5 with radius 0.5 -> top of ball at y ~ 1.0
    // Origin at y = 20.0 -> distance should be approx 19.0
    assert!(
        (dist - 19.0).abs() < 1.0,
        "Distance to ball top should be ~19.0, got {}",
        dist
    );
}
