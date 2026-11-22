//! Interpolation rules: the blend, and the three cases that are not a blend.

use super::*;
use crate::state::{ClientState, Entity, Value};
use engine_schema::{RootCollection, RootState, SchemaDef, SchemaField, WireKind, emit_descriptor};

fn field(name: &str, kind: WireKind) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding: engine_schema::default_encoding(kind),
        nested_path: None,
        line: 0,
        column: 0,
    }
}

/// Player: [0] name Str, [1] x F64, [2] y F64, [3] alive Bool.
fn descriptor() -> Descriptor {
    let player = SchemaDef {
        nested_arrays: Vec::new(),
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("name", WireKind::String),
            field("x", WireKind::F64),
            field("y", WireKind::F64),
            field("alive", WireKind::Bool),
        ],
        line: 0,
        column: 0,
    };
    let root = RootState {
        nested: Vec::new(),
        name: "Arena".into(),
        version: 1,
        scalars: vec![],
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    engine_schema::parse_descriptor(&emit_descriptor(&[player], Some(&root))).expect("descriptor")
}

fn state_at(d: &Descriptor, rows: &[(u32, f64, f64)]) -> ClientState {
    let mut state = ClientState::new(d);
    let collection = state.collections.first_mut().expect("a collection");
    collection.entities.clear();
    for &(id, x, y) in rows {
        collection.entities.insert(
            id,
            Entity {
                id,
                values: vec![
                    Value::Str("p".into()),
                    Value::F64(x),
                    Value::F64(y),
                    Value::Bool(true),
                ],
            },
        );
    }
    state
}

fn sample(interp: &mut Interp, id: u32, t: u32) -> Option<Vec<f32>> {
    let mut out = Vec::new();
    interp.sample(id, t, &mut out).then_some(out)
}

#[test]
fn blends_between_the_two_bracketing_snapshots() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(10, &state_at(&d, &[(1, 0.0, 0.0)]), &d);
    interp.push(12, &state_at(&d, &[(1, 10.0, 20.0)]), &d);

    let mid = sample(&mut interp, 1, 11).expect("entity is in the ring");
    assert_eq!(mid[1], 5.0, "x is halfway");
    assert_eq!(mid[2], 10.0, "y is halfway");
    assert!(mid[0].is_nan(), "a string field does not blend");

    assert_eq!(interp.stats().blended, 1);

    // An exact tick lands on a real snapshot, so it is answered as-is rather than blended.
    let at_tick = sample(&mut interp, 1, 12).expect("exact tick");
    assert_eq!(at_tick[1], 10.0);
    assert_eq!(interp.stats().snapped, 1);
}

#[test]
fn a_single_snapshot_is_drawn_as_it_is() {
    let d = descriptor();
    let mut interp = Interp::new(2);
    interp.push(10, &state_at(&d, &[(1, 3.0, 4.0)]), &d);

    let out = sample(&mut interp, 1, 10).expect("one snapshot is enough to draw");
    assert_eq!(out[1], 3.0);
    assert_eq!(interp.stats().snapped, 1, "counted as a pop, not a blend");
}

#[test]
fn a_tick_older_than_the_ring_clamps_to_the_oldest() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(20, &state_at(&d, &[(1, 1.0, 0.0)]), &d);
    interp.push(21, &state_at(&d, &[(1, 9.0, 0.0)]), &d);

    let out = sample(&mut interp, 1, 5).expect("clamped, not dropped");
    assert_eq!(out[1], 1.0);
    assert_eq!(interp.stats().clamped, 1);
}

#[test]
fn a_tick_past_the_newest_holds_instead_of_extrapolating() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(20, &state_at(&d, &[(1, 1.0, 0.0)]), &d);
    interp.push(21, &state_at(&d, &[(1, 9.0, 0.0)]), &d);

    let out = sample(&mut interp, 1, 99).expect("held at the newest");
    assert_eq!(out[1], 9.0);
    assert_eq!(interp.stats().held, 1);
    assert_eq!(interp.stats().jumps, 0, "holding is not a jump");
}

#[test]
fn a_removed_entity_leaves_the_ring_and_does_not_ghost() {
    let d = descriptor();
    let mut interp = Interp::new(4);
    interp.push(10, &state_at(&d, &[(1, 0.0, 0.0), (2, 5.0, 5.0)]), &d);
    interp.push(11, &state_at(&d, &[(2, 6.0, 6.0)]), &d);

    assert!(
        sample(&mut interp, 1, 8).is_none(),
        "gone from the ring at once"
    );
    assert!(sample(&mut interp, 2, 8).is_some());
    assert_eq!(interp.stats().despawns, 1);
    assert_eq!(interp.entities(), vec![2]);
}

#[test]
fn a_late_older_tick_is_ignored_rather_than_sorted_in() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(10, &state_at(&d, &[(1, 1.0, 0.0)]), &d);
    interp.push(12, &state_at(&d, &[(1, 3.0, 0.0)]), &d);
    let before = interp.newest_tick();
    interp.push(9, &state_at(&d, &[(1, 100.0, 0.0)]), &d);
    assert_eq!(interp.newest_tick(), before, "the ring stays ascending");
}

#[test]
fn a_repeated_tick_replaces_its_snapshot() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(10, &state_at(&d, &[(1, 1.0, 0.0)]), &d);
    interp.push(10, &state_at(&d, &[(1, 7.0, 0.0)]), &d);

    let out = sample(&mut interp, 1, 10).expect("one tick, one snapshot");
    assert_eq!(out[1], 7.0, "the later section for the same tick wins");
}

#[test]
fn the_render_tick_trails_the_newest_by_the_delay() {
    let d = descriptor();
    let mut interp = Interp::new(3);
    interp.push(50, &state_at(&d, &[(1, 0.0, 0.0)]), &d);
    assert_eq!(interp.newest_tick(), Some(50));
    assert_eq!(interp.render_tick(), Some(47));
    interp.set_delay_ticks(1);
    assert_eq!(interp.render_tick(), Some(49));
}

#[test]
fn a_teleport_is_counted_as_a_jump() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    interp.push(10, &state_at(&d, &[(1, 0.0, 0.0)]), &d);
    interp.push(11, &state_at(&d, &[(1, 50.0, 0.0)]), &d);

    sample(&mut interp, 1, 10).expect("first sample");
    sample(&mut interp, 1, 11).expect("second sample");

    let stats = interp.stats();
    assert_eq!(stats.jumps, 1, "a 50-unit step is not smooth motion");
    assert_eq!(stats.max_step, 50.0);
}

#[test]
fn history_is_bounded() {
    let d = descriptor();
    let mut interp = Interp::new(0);
    for tick in 0..(HISTORY_TICKS as u32 + 20) {
        interp.push(tick, &state_at(&d, &[(1, tick as f64, 0.0)]), &d);
    }
    assert_eq!(interp.ticks.len(), HISTORY_TICKS);
    assert_eq!(interp.newest_tick(), Some(HISTORY_TICKS as u32 + 19));
}
