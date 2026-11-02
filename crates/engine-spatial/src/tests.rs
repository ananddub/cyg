use super::*;

#[test]
fn test_spatial_index_2d_radius_and_nearest() {
    let mut idx = SpatialIndex2D::new();
    idx.insert(1, [0.0, 0.0]);
    idx.insert(2, [10.0, 0.0]);
    idx.insert(3, [50.0, 50.0]);

    let hits = idx.query_radius([0.0, 0.0], 15.0);
    let ids: Vec<u32> = hits.iter().map(|h| h.entity_id).collect();
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(!ids.contains(&3));

    let nearest = idx.nearest([8.0, 1.0]).unwrap();
    assert_eq!(nearest.entity_id, 2);
}

#[test]
fn test_spatial_index_3d_radius_and_aoi() {
    let mut idx = SpatialIndex3D::new();
    idx.insert(101, [0.0, 1.8, 0.0]);
    idx.insert(102, [20.0, 1.8, 0.0]);
    idx.insert(103, [100.0, 1.8, 100.0]);

    let nearby = idx.query_radius([0.0, 1.8, 0.0], 30.0);
    assert_eq!(nearby.len(), 2);
    let ids: Vec<u32> = nearby.iter().map(|h| h.entity_id).collect();
    assert!(ids.contains(&101));
    assert!(ids.contains(&102));
    assert!(!ids.contains(&103));
}

#[test]
fn test_aoi_manager_3d_visibility_culling() {
    let mut aoi = AoiManager3D::new();
    aoi.update_entity(1, [0.0, 0.0, 0.0]);
    aoi.update_entity(2, [35.0, 0.0, 0.0]);
    aoi.update_entity(3, [250.0, 0.0, 0.0]);

    let visible_to_1 = aoi.get_visible_entities(1, 50.0);
    assert_eq!(visible_to_1, vec![2]);

    let visible_to_3 = aoi.get_visible_entities(3, 50.0);
    assert!(visible_to_3.is_empty());

    aoi.update_entity(2, [300.0, 0.0, 0.0]);
    let visible_to_1_after_move = aoi.get_visible_entities(1, 50.0);
    assert!(visible_to_1_after_move.is_empty());
}

#[test]
fn test_aoi_visibility_delta_entered_and_left_3d() {
    let mut aoi = AoiManager3D::new();

    let d1 = aoi.update_entity_with_delta(1, [0.0, 0.0, 0.0], 50.0);
    assert!(d1.entered.is_empty());
    assert!(d1.left.is_empty());

    aoi.update_entity(2, [25.0, 0.0, 0.0]);
    let d2 = aoi.compute_visibility_delta(1, 50.0);
    assert_eq!(d2.entered, vec![2]);
    assert!(d2.left.is_empty());

    aoi.update_entity(2, [200.0, 0.0, 0.0]);
    let d3 = aoi.compute_visibility_delta(1, 50.0);
    assert!(d3.entered.is_empty());
    assert_eq!(d3.left, vec![2]);

    let d4_2 = aoi.update_entity_with_delta(2, [10.0, 0.0, 0.0], 50.0);
    assert_eq!(d4_2.entered, vec![1]);
    let d4 = aoi.compute_visibility_delta(1, 50.0);
    assert_eq!(d4.entered, vec![2]);
    assert!(d4.left.is_empty());

    let remove_delta = aoi.remove_entity(2);
    assert_eq!(remove_delta.left, vec![1]);
    let d5 = aoi.compute_visibility_delta(1, 50.0);
    assert_eq!(d5.left, vec![2]);
}

#[test]
fn test_aoi_repeated_idle_ticks_then_exit() {
    let mut aoi = AoiManager3D::new();
    aoi.update_entity(1, [0.0, 0.0, 0.0]);
    aoi.update_entity(2, [10.0, 0.0, 0.0]);

    assert_eq!(
        aoi.compute_visibility_delta(1, 50.0),
        VisibilityDelta {
            entered: vec![2],
            left: vec![]
        }
    );
    for _ in 0..8 {
        aoi.update_entity(1, [0.5, 0.0, 0.0]);
        assert_eq!(
            aoi.compute_visibility_delta(1, 50.0),
            VisibilityDelta::default()
        );
    }

    aoi.update_entity(2, [400.0, 0.0, 0.0]);
    assert_eq!(
        aoi.compute_visibility_delta(1, 50.0),
        VisibilityDelta {
            entered: vec![],
            left: vec![2]
        }
    );
}

#[test]
fn test_aoi_visibility_delta_2d() {
    let mut aoi = AoiManager2D::new();
    aoi.update_entity(10, [0.0, 0.0]);
    aoi.update_entity(20, [30.0, 0.0]);

    let d1 = aoi.compute_visibility_delta(10, 40.0);
    assert_eq!(d1.entered, vec![20]);

    aoi.update_entity(20, [100.0, 100.0]);
    let d2 = aoi.compute_visibility_delta(10, 40.0);
    assert_eq!(d2.left, vec![20]);
}
