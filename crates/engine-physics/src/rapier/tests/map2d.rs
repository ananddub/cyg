use crate::rapier::RapierWorld2D;

#[test]
fn from_json_loads_a_static_2d_arena() {
    let json = r#"{
        "arena_width": 800,
        "arena_height": 500,
        "gravity": [0, -19.6],
        "obstacles": [
            { "id": 1, "name": "wall", "x": 400, "y": 250, "w": 40, "h": 200 },
            { "id": 2, "name": "rock", "x": 100, "y": 100, "r": 12 }
        ]
    }"#;
    let world = RapierWorld2D::from_json(json).expect("parse 2D map");

    assert_eq!(world.arena_width, 800.0);
    assert_eq!(world.arena_height, 500.0);
    assert_eq!(world.gravity.y, -19.6, "gravity override honoured");
    assert_eq!(world.collider_set.len(), 2, "one collider per obstacle");

    let mut stepped = world;
    for _ in 0..30 {
        stepped.step();
    }
    let bodies: Vec<_> = stepped
        .rigid_body_set
        .iter()
        .map(|(_, b)| b.is_fixed())
        .collect();
    assert!(bodies.iter().all(|fixed| *fixed), "map geometry is static");

    let hit = stepped.raycast_hit([200.0, 250.0], [1.0, 0.0], 500.0);
    let (entity, toi) = hit.expect("wall is on the ray");
    assert!(
        toi > 150.0 && toi < 250.0,
        "hit before the wall plane, toi={toi}"
    );
    assert_eq!(entity, 0, "static geometry reports entity 0");
}

#[test]
fn rotated_box_changes_the_collision_footprint() {
    let json = r#"{
        "arena_width": 400, "arena_height": 400,
        "obstacles": [
            { "id": 1, "x": 200, "y": 200, "w": 20, "h": 200, "rotation": 1.5707964 }
        ]
    }"#;
    let mut world = RapierWorld2D::from_json(json).expect("parse");

    let through_band = world.raycast_hit([150.0, 300.0], [0.0, -1.0], 300.0);
    assert!(through_band.is_some(), "a vertical ray crosses the y-band");
    let above_band_short = world.raycast_hit([150.0, 300.0], [1.0, 0.0], 45.0);
    assert!(
        above_band_short.is_none(),
        "x-ray stops before the band starts at x=100"
    );

    let across_band = world.raycast_hit([50.0, 200.0], [1.0, 0.0], 300.0);
    let (entity, toi) = across_band.expect("x-ray at y=200 reaches the band");
    assert_eq!(entity, 0, "static map geometry");
    assert!(
        (toi - 50.0).abs() < 1.0,
        "first contact at the band edge, toi={toi}"
    );
}
