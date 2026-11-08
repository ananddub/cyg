use super::*;

#[test]
fn tick_timers_fire_and_rearm_deterministically() {
    let mut host = RoomHostData::new(PhysicsWorld3D::default());
    host.timer_set(1, 5, 0);
    host.timer_set(2, 3, 4);

    assert_eq!(host.timer_poll(), 0, "nothing due before any tick");
    host.clock_ticks = 3;
    assert_eq!(host.timer_poll(), 2, "interval timer fires at tick 3");
    assert_eq!(host.timer_poll(), 0);
    host.clock_ticks = 5;
    assert_eq!(host.timer_poll(), 1, "one-shot fires at tick 5");
    assert_eq!(
        host.timer_poll(),
        0,
        "interval re-armed for 3+4=7, not due at 5"
    );
    host.clock_ticks = 7;
    assert_eq!(host.timer_poll(), 2, "interval refires at 7");
    host.timer_cancel(2);
    host.clock_ticks = 100;
    assert_eq!(host.timer_poll(), 0, "cancelled timer stays silent");
}

#[test]
fn rapier3d_family_simulates_and_reports_contacts() {
    let mut host = RoomHostData::new(PhysicsWorld3D::default());
    host.init_rapier3d(0.0, -9.81, 0.0);
    let a = host.rapier3d_create_body(0, 0.0, 5.0, 0.0);
    let b = host.rapier3d_create_body(1, 0.0, 0.0, 0.0);
    host.rapier3d_add_ball_collider(a, 0.5);
    host.rapier3d_add_ball_collider(b, 0.5);

    assert!(!host.rapier3d_check_contact(a, b));
    for _ in 0..70 {
        host.rapier3d_step(1.0 / 60.0);
    }
    let by = host.rapier3d_get_position(b).unwrap()[1];
    assert!(
        host.rapier3d_get_position(a).unwrap()[1] < 1.05,
        "ball settled onto the anvil (by={by})"
    );
    host.rapier3d_set_position(a, 0.0, 0.9, 0.0);
    host.rapier3d_step(1.0 / 60.0);
    assert!(
        host.rapier3d_check_contact(a, b),
        "overlapping bodies report contact"
    );
    assert!(host.rapier3d_get_position(a).unwrap()[1] > host.rapier3d_get_position(b).unwrap()[1]);

    let (ay, by) = (
        host.rapier3d_get_position(a).unwrap()[1],
        host.rapier3d_get_position(b).unwrap()[1],
    );
    tracing::info!(
        "DEBUG ay={ay} by={by} contact={}",
        host.rapier3d_check_contact(a, b)
    );
    host.rapier3d_remove_body(b);
    assert!(!host.rapier3d_check_contact(a, b));
}

#[test]
fn spatial_remove_drops_the_entity() {
    let mut host = RoomHostData::new(PhysicsWorld3D::default());
    host.spatial.upsert(7, [10.0, 10.0]);
    assert!(
        host.spatial
            .query_radius([10.0, 10.0], 1.0)
            .iter()
            .any(|h| h.entity_id == 7)
    );
    host.spatial.remove(7, [10.0, 10.0]);
    assert!(
        !host
            .spatial
            .query_radius([10.0, 10.0], 1.0)
            .iter()
            .any(|h| h.entity_id == 7)
    );
}

#[test]
fn navmesh_find_path_routes_across_the_mesh() {
    let mesh = engine_nav::nav2d::NavMesh2D::from_trimesh(
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        vec![[0, 1, 2], [0, 2, 3]],
    )
    .expect("quad navmesh");
    let mut host = RoomHostData::new(PhysicsWorld3D::default());
    assert_eq!(
        host.navmesh_find_path((1.0, 1.0), (9.0, 9.0)),
        None,
        "no navmesh injected"
    );
    host.navmesh = Some(std::sync::Arc::new(mesh));
    let path = host
        .navmesh_find_path((1.0, 1.0), (9.0, 9.0))
        .expect("path across the quad");
    assert!(!path.is_empty(), "at least the goal waypoint");
    let (last_x, last_y) = path[path.len() - 1];
    assert!(
        (last_x - 9.0).abs() < 2.0 && (last_y - 9.0).abs() < 2.0,
        "path ends near the goal"
    );
}
