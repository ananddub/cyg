use engine_config::AoiConfig;

use super::filter_for_viewer;
use super::parse_delta;
use super::support::{synthetic_delta, test_aoi};

#[test]
fn near_kept_every_tick_far_only_on_skip_boundary() {
    let d = synthetic_delta();
    let (.., entities, bc, range) = parse_delta(&d).unwrap();

    let frange = (range.0 + 4, range.1);
    let aoi = test_aoi();
    let mut out = Vec::new();

    filter_for_viewer(
        &entities,
        bc,
        frange,
        &d,
        1,
        Some([0.; 3]),
        3,
        &aoi,
        &mut out,
    );
    let (ver, kept, kept_bc, _) = parse_delta(&out).unwrap();
    assert_eq!(ver, 1);
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().any(|e| e.id == 1));
    assert!(kept.iter().any(|e| e.id == 2));
    assert!(!kept.iter().any(|e| e.id == 3));

    assert_eq!(kept_bc, 1);
    let new_bc_off = 5 + kept.len() * 72;
    let owner = i64::from_le_bytes(
        out[new_bc_off + 4 + 24..new_bc_off + 4 + 32]
            .try_into()
            .unwrap(),
    );
    assert_eq!(owner, 1);

    filter_for_viewer(
        &entities,
        bc,
        frange,
        &d,
        1,
        Some([0.; 3]),
        4,
        &aoi,
        &mut out,
    );
    let (_, kept, kept_bc, _) = parse_delta(&out).unwrap();
    assert_eq!(kept.len(), 3);
    assert_eq!(kept_bc, 1);
}

#[test]
fn skip_ticks_one_sends_far_every_tick() {
    let d = synthetic_delta();
    let (_, entities, bc, range) = parse_delta(&d).unwrap();
    let aoi = AoiConfig {
        skip_ticks: 1,
        ..test_aoi()
    };
    let mut out = Vec::new();
    filter_for_viewer(
        &entities,
        bc,
        (range.0 + 4, range.1),
        &d,
        1,
        Some([0.; 3]),
        3,
        &aoi,
        &mut out,
    );
    let (_, kept, _, _) = parse_delta(&out).unwrap();
    assert_eq!(kept.len(), 3);
}

#[test]
fn no_viewer_pos_sends_everything() {
    let d = synthetic_delta();
    let (_, entities, bc, range) = parse_delta(&d).unwrap();
    let mut out = Vec::new();
    filter_for_viewer(
        &entities,
        bc,
        (range.0 + 4, range.1),
        &d,
        1,
        None,
        3,
        &test_aoi(),
        &mut out,
    );
    let (_, kept, kept_bc, _) = parse_delta(&out).unwrap();
    assert_eq!(kept.len(), 3);
    assert_eq!(kept_bc, 2);
}
