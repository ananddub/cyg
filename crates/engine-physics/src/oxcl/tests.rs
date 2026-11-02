use super::*;
use crate::rapier::RapierWorld3D;
use rapier3d::prelude as r3d;

#[test]
fn oxcl_roundtrip_and_trimesh() {
    let mesh = CookedMesh {
        vertices: vec![
            [-50.0, 0.0, -50.0],
            [50.0, 0.0, -50.0],
            [50.0, 0.0, 50.0],
            [-50.0, 0.0, 50.0],
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let bytes = mesh.to_oxcl_bytes();
    let loaded = CookedMesh::from_oxcl_bytes(&bytes).expect("valid oxcl");
    assert_eq!(loaded, mesh);

    let mut world = RapierWorld3D::default();
    world.add_oxcl_mesh(&loaded);
    let body = r3d::RigidBodyBuilder::dynamic()
        .translation(r3d::Vector::new(0.0, 5.0, 0.0))
        .build();
    let handle = world.rigid_body_set.insert(body);
    let col = r3d::ColliderBuilder::ball(0.5).build();
    world
        .collider_set
        .insert_with_parent(col, handle, &mut world.rigid_body_set);
    for _ in 0..120 {
        world.step();
    }
    let y = world.rigid_body_set[handle].translation().y;
    assert!(
        y < 1.5 && y >= 0.40,
        "ball must rest on the OXCL arena, got {y}"
    );

    let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "ray down must hit the OXCL arena");
}

#[test]
fn doc_style_cuboid_raycast_works() {
    let mut world = RapierWorld3D::default();
    let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    world.collider_set.insert(ground);
    let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "cuboid baseline raycast");
}

#[test]
fn trimesh_raycast_isolated() {
    let mesh = CookedMesh {
        vertices: vec![[-50.0, 0.0, -50.0], [50.0, 0.0, -50.0], [50.0, 0.0, 50.0]],
        indices: vec![0, 1, 2],
    };
    let mut world = RapierWorld3D::default();
    world.add_oxcl_mesh(&mesh);

    let hit = world.raycast_hit([0.0, 10.0, 0.0], [0.0, -1.0, 0.0], 100.0);
    assert!(hit.is_some(), "trimesh raycast (lazy sync)");
}

#[test]
fn raycast_minimal_debug() {
    let shape = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0).build();
    let ray = r3d::Ray::new(
        r3d::Vector::new(0.0, 10.5, 0.0),
        r3d::Vector::new(0.0, -1.0, 0.0),
    );
    let hit = shape.shape().cast_local_ray(&ray, 100.0, true);
    assert!(hit.is_some(), "direct local cuboid cast, got {hit:?}");

    let mut world = RapierWorld3D::default();
    let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
        .translation(r3d::Vector::new(0.0, -0.5, 0.0))
        .build();
    world.collider_set.insert(ground);
    let world_ray = r3d::Ray::new(
        r3d::Vector::new(0.0, 10.0, 0.0),
        r3d::Vector::new(0.0, -1.0, 0.0),
    );
    for (h, col) in world.collider_set.iter() {
        let _ = h;

        let pos = col.position();
        let local = world_ray.transform_by(&pos.inverse());
        let hit = col.shape().cast_local_ray(&local, 100.0, true);
        assert!(hit.is_some(), "position-inverse cuboid cast, got {hit:?}");
    }
}
