use super::parse_delta;
use super::support::synthetic_delta;
use super::viewer_pos_of;

#[test]
fn parse_round_trip() {
    let d = synthetic_delta();
    let (ver, entities, bc, range) = parse_delta(&d).expect("parses");
    assert_eq!(ver, 1);
    assert_eq!(entities.len(), 3);
    assert_eq!(entities[0].id, 1);
    assert_eq!(entities[1].pos, [50.0, 0.0, 0.0]);
    assert_eq!(bc, 2);
    assert_eq!(range, (5 + 3 * 72, 5 + 3 * 72 + 4 + 2 * 32));
    assert_eq!(range.1, d.len());
}

#[test]
fn parse_rejects_truncation_and_bad_version() {
    let d = synthetic_delta();
    assert!(parse_delta(&d[..d.len() - 1]).is_none());
    assert!(parse_delta(&d[..10]).is_none());
    assert!(parse_delta(&[]).is_none());
    let mut bad = d.clone();
    bad[0] = 2;
    assert!(parse_delta(&bad).is_none());
}

#[test]
fn viewer_pos_of_finds_self_record() {
    let d = synthetic_delta();
    let (_, entities, ..) = parse_delta(&d).unwrap();
    assert_eq!(viewer_pos_of(&entities, 2), Some([50.0, 0.0, 0.0]));
    assert_eq!(viewer_pos_of(&entities, 99), None);
}
