use bevy_ecs::prelude::*;
use engine_core::{EntityId, PendingDespawn, Position2D, TickRunner, Velocity};

#[derive(Resource, Default)]
struct ChangedLog(Vec<u32>);

fn movement_system(mut query: Query<(&mut Position2D, &Velocity)>) {
    for (mut pos, vel) in &mut query {
        pos.x += vel.vx;
        pos.y += vel.vy;
    }
}

#[test]
fn test_ecs_basic_spawn_and_tick() {
    let mut runner = TickRunner::new();
    runner.add_system(movement_system);

    // Spawn 10,000 entities
    const ENTITY_COUNT: u32 = 10_000;
    for i in 0..ENTITY_COUNT {
        runner.world.spawn((
            EntityId(i),
            Position2D { x: 0.0, y: 0.0 },
            Velocity {
                vx: 1.0,
                vy: 2.0,
                vz: 0.0,
            },
        ));
    }

    // Tick 10 times
    for _ in 0..10 {
        runner.tick();
    }

    // Verify positions: x should be 10.0, y should be 20.0
    let mut count = 0;
    let mut query = runner.world.query::<(&EntityId, &Position2D)>();
    for (_id, pos) in query.iter(&runner.world) {
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 20.0);
        count += 1;
    }
    assert_eq!(count, ENTITY_COUNT);
}

#[test]
fn test_ecs_changed_tracking() {
    let mut runner = TickRunner::new();

    let e1 = runner.world.spawn((
        EntityId(1),
        Position2D { x: 0.0, y: 0.0 },
    )).id();

    let _e2 = runner.world.spawn((
        EntityId(2),
        Position2D { x: 5.0, y: 5.0 },
    )).id();

    // System that checks Changed<Position2D>
    fn delta_tracker(
        query: Query<(&EntityId, &Position2D), Changed<Position2D>>,
        mut changed_log: ResMut<ChangedLog>,
    ) {
        for (id, _pos) in &query {
            changed_log.0.push(id.0);
        }
    }

    runner.world.insert_resource(ChangedLog::default());
    runner.add_system(delta_tracker);

    // First tick: Newly spawned components are marked as Changed
    runner.tick();
    {
        let log = runner.world.resource::<ChangedLog>();
        assert_eq!(log.0.len(), 2);
    }

    // Clear log and tick again without mutations
    runner.world.resource_mut::<ChangedLog>().0.clear();
    runner.tick();
    {
        let log = runner.world.resource::<ChangedLog>();
        assert_eq!(log.0.len(), 0, "No components changed, should detect 0 changes");
    }

    // Mutate only e1
    {
        let mut e1_mut = runner.world.entity_mut(e1);
        let mut pos = e1_mut.get_mut::<Position2D>().unwrap();
        pos.x += 10.0;
    }

    runner.tick();
    {
        let log = runner.world.resource::<ChangedLog>();
        assert_eq!(log.0.as_slice(), &[1], "Only Entity 1 should be detected as changed");
    }
}

#[test]
fn test_ecs_pending_despawn_cleanup() {
    let mut runner = TickRunner::new();

    let e1 = runner.world.spawn((EntityId(1), Position2D::default())).id();
    let e2 = runner.world.spawn((EntityId(2), Position2D::default())).id();

    // Mark e1 for despawn
    runner.world.entity_mut(e1).insert(PendingDespawn);

    runner.tick();

    assert!(runner.world.get_entity(e1).is_err(), "e1 should have been despawned");
    assert!(runner.world.get_entity(e2).is_ok(), "e2 should still exist");
}
