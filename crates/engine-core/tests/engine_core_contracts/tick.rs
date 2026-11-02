use bevy_ecs::prelude::*;
use engine_core::*;

#[test]
fn tick_runner_empty_schedule_is_a_noop_tick() {
    let mut runner = TickRunner::new();
    for _ in 0..4 {
        runner.tick();
    }
    assert_eq!(runner.world.entities().len(), 0);
}

#[test]
fn tick_runner_despawns_every_marked_entity() {
    let mut runner = TickRunner::new();
    for _ in 0..50 {
        runner
            .world
            .spawn(Position2D::default())
            .insert(PendingDespawn);
    }
    runner.world.spawn(Position2D::default());
    runner.tick();
    assert_eq!(runner.world.entities().len(), 1);
}

#[test]
fn tick_runner_survivor_when_marker_removed_before_tick() {
    let mut runner = TickRunner::new();
    let marked = runner
        .world
        .spawn(Position2D::default())
        .insert(PendingDespawn)
        .id();
    runner
        .world
        .entity_mut(marked)
        .remove::<PendingDespawn>();
    runner.tick();
    assert!(runner.world.get_entity(marked).is_ok());
}

#[test]
fn tick_runner_tolerates_entity_despawned_by_a_system() {
    let mut runner = TickRunner::new();
    let victim = runner
        .world
        .spawn(Position2D::default())
        .insert(PendingDespawn)
        .id();
    runner.add_system(move |mut commands: Commands, q: Query<Entity, With<Position2D>>| {
        for e in &q {
            if e == victim {
                commands.entity(e).despawn();
            }
        }
    });
    runner.tick();
    assert_eq!(runner.world.entities().len(), 0);
}
