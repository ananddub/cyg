use super::*;
use engine_config::{FeaturesConfig, PhysicsFeature};

#[test]
fn physics_none_allocates_no_rapier_worlds_and_guest_calls_are_quiet_zeros() {
    let mut data = RoomHostData::new_with_features(
        PhysicsWorld3D::default(),
        &FeaturesConfig {
            physics: PhysicsFeature::None,
            voice: true,
        },
    );
    assert!(data.rapier2d.is_none(), "2D world never born");
    assert!(data.rapier3d.is_none(), "3D world never born");

    // A guest ignoring the config and calling the ABI gets quiet zeros,
    // and the world is never resurrected by init.
    assert_eq!(data.rapier2d_create_body(1, 1.0, 1.0), 0);
    assert_eq!(data.rapier3d_create_body(1, 1.0, 1.0, 1.0), 0);
    data.init_rapier2d(0.0, -9.8);
    data.init_rapier3d(0.0, -9.8, 0.0);
    assert!(data.rapier2d.is_none() && data.rapier3d.is_none());
    assert!(!data.rapier2d_check_contact(1, 2));
    assert!(!data.rapier3d_check_contact(1, 2));
    assert_eq!(data.rapier2d_get_velocity(1), (0.0, 0.0));
    assert_eq!(data.rapier3d_get_velocity(1), [0.0; 3]);
}

#[test]
fn physics_2d_only_skips_the_3d_world_but_runs_2d() {
    let mut data = RoomHostData::new_with_features(
        PhysicsWorld3D::default(),
        &FeaturesConfig {
            physics: PhysicsFeature::D2,
            voice: true,
        },
    );
    assert!(data.rapier2d.is_some());
    assert!(
        data.rapier3d.is_none(),
        "3D world skipped for a 2D-only game"
    );

    let body = data.rapier2d_create_body(1, 0.0, 5.0);
    assert!(body > 0, "the requested backend works normally");
    data.rapier2d_step(1.0 / 60.0);
    assert!(data.rapier3d_create_body(1, 0.0, 0.0, 0.0) == 0);
}

#[test]
fn default_features_keep_both_worlds_alive() {
    let data = RoomHostData::new(PhysicsWorld3D::default());
    assert!(data.rapier2d.is_some() && data.rapier3d.is_some());
    assert!(data.features.wants_rapier2d() && data.features.wants_rapier3d());
}

#[test]
fn a_guest_importing_a_denied_backend_is_refused_at_boot() {
    use crate::host::assert_features_support_module;

    let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
    let rapier2d_guest = runtime
        .compile(r#"(module (import "cyg" "sys_rapier2d_init" (func (param f64 f64))))"#.as_bytes())
        .expect("wat compiles");
    let rapier3d_guest = runtime
        .compile(
            r#"(module (import "cyg" "sys_rapier3d_init" (func (param f64 f64 f64))))"#.as_bytes(),
        )
        .expect("wat compiles");
    let logic_guest = runtime
        .compile(r#"(module (import "cyg" "on_alloc_stub" (func)))"#.as_bytes())
        .expect("wat compiles");

    let none = FeaturesConfig {
        physics: PhysicsFeature::None,
        voice: true,
    };
    let d2 = FeaturesConfig {
        physics: PhysicsFeature::D2,
        voice: true,
    };

    let err = assert_features_support_module(&none, &rapier2d_guest)
        .expect_err("rapier2d import must fail under physics=none");
    assert!(err.contains("2D"), "{err}");
    let err = assert_features_support_module(&none, &rapier3d_guest)
        .expect_err("rapier3d import must fail under physics=none");
    assert!(err.contains("3D"), "{err}");
    let err = assert_features_support_module(&d2, &rapier3d_guest)
        .expect_err("rapier3d import must fail under physics=2d");
    assert!(err.contains("3D"), "{err}");

    assert!(
        assert_features_support_module(&d2, &rapier2d_guest).is_ok(),
        "the requested backend imports fine"
    );
    assert!(
        assert_features_support_module(&none, &logic_guest).is_ok(),
        "a guest with no rapier imports boots under physics=none"
    );
}
