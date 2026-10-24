use engine_nav::{NavMesh2D, NavMesh3D};

#[test]
fn test_nav2d_from_trimesh_and_pathfinding() {
    // Define a 10x10 square divided into 2 triangles
    // (0, 10) --- (10, 10)
    //    |    \      |
    //    |      \    |
    // (0, 0)  --- (10, 0)
    let vertices = vec![
        (0.0, 0.0),   // 0
        (10.0, 0.0),  // 1
        (10.0, 10.0), // 2
        (0.0, 10.0),  // 3
    ];
    let triangles = vec![
        [0, 1, 2], // Tri 1
        [0, 2, 3], // Tri 2
    ];

    let nav = NavMesh2D::from_trimesh(vertices, triangles).expect("Valid mesh");

    // Path from (1.0, 1.0) to (9.0, 9.0)
    let path = nav.find_path((1.0, 1.0), (9.0, 9.0));
    assert!(path.is_some(), "Path should be found across the mesh");
    let waypoints = path.unwrap();
    println!("Found 2D waypoints: {:?}", waypoints);
    assert!(!waypoints.is_empty());
    assert_eq!(waypoints.last().unwrap(), &(9.0, 9.0));

    // Path to a point outside the mesh should return None
    let outside_path = nav.find_path((1.0, 1.0), (50.0, 50.0));
    assert!(outside_path.is_none(), "Unreachable points outside mesh must return None");
}

#[test]
fn test_nav3d_recast_bounds() {
    let nav3d = NavMesh3D::new();
    // Default config has AABB
    let inside = [0.0, 0.0, 0.0];
    let _ = nav3d.is_in_bounds(inside);
    assert_eq!(nav3d.baked_data.len(), 0);

    let baked = NavMesh3D::from_baked(vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(baked.baked_data.len(), 3);
}
