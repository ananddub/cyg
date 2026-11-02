use bevy_ecs::prelude::*;
use engine_core::*;

#[test]
fn health_overkill_clamps_at_zero_and_dies() {
    let mut hp = Health::new(100.0);
    assert!(hp.take_damage(250.0));
    assert!(hp.is_dead());
    assert_eq!(hp.current, 0.0);
    assert_eq!(hp.max, 100.0);
}

#[test]
fn health_death_boundary_is_exactly_zero() {
    let mut hp = Health::new(10.0);
    assert!(hp.take_damage(10.0));
    assert!(hp.is_dead());
}

#[test]
fn health_negative_damage_heals_never_past_max() {
    let mut hp = Health::new(50.0);
    hp.take_damage(30.0);
    assert_eq!(hp.current, 20.0);
    hp.take_damage(-100.0);
    assert_eq!(hp.current, 50.0);
    assert!(!hp.is_dead());
}

#[test]
fn components_storage_roundtrip() {
    let mut world = World::new();
    let e = world
        .spawn((
            EntityId(7),
            Position3D::default(),
            Velocity::default(),
            PlayerControlled { client_id: 9001 },
            Health::new(100.0),
        ))
        .id();
    let (id, pos, vel, pc, hp) = world
        .entity(e)
        .get_components::<(
            &EntityId,
            &Position3D,
            &Velocity,
            &PlayerControlled,
            &Health,
        )>()
        .unwrap();
    assert_eq!(id.0, 7);
    assert_eq!(*pos, Position3D::default());
    assert_eq!(*vel, Velocity::default());
    assert_eq!(pc.client_id, 9001);
    assert_eq!(hp.current, 100.0);
}

#[test]
#[should_panic(expected = "tick_rate must be > 0")]
fn fixed_clock_zero_rate_panics_with_clear_message() {
    let _ = FixedClock::new(0);
}

#[test]
fn fixed_clock_step_values_match_rate() {
    let mut clock = FixedClock::new(128);
    assert_eq!(clock.tick_rate, 128);
    let ctx = clock.advance();
    assert_eq!(ctx.tick, 1);
    assert!((ctx.dt - 1.0 / 128.0).abs() < 1e-6);
    assert!((ctx.dt_ms - 7.8125).abs() < 1e-9);
    let ctx = clock.advance();
    assert_eq!(ctx.tick, 2);
}
