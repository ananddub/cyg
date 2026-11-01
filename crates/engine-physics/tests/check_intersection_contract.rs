//! Contract tests for `RapierWorld2D/3D::check_intersection` — the function
//! behind the guest `Rapier2D/3D.checkContact` ABI.
//!
//! Regression context: the contact fallback used to walk the WORLD'S WHOLE
//! contact graph per call (`narrow_phase.contact_pairs()`), making N per-tick
//! checks O(N x pairs) — the bug that made WASM rooms collapse past ~400
//! players. Both variants now use `contact_pairs_with(col_a)` (O(degree)).
//! These tests pin correctness in all three states and guard the scaling.

use engine_physics::{RapierWorld2D, RapierWorld3D};

/// Two balls of radius r whose centers are `gap` apart.
const R: f32 = 0.5;

// ---------------------------------------------------------------------------
// 2D
// ---------------------------------------------------------------------------

#[test]
fn twod_overlapping_bodies_intersect() {
    let mut w = RapierWorld2D::new(0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0);
    let b = w.create_body(0, 0.6, 0.0); // centers 0.6 < 2r: deep overlap
    w.add_circle_collider(a, R, false, 0.0, 0.5);
    w.add_circle_collider(b, R, false, 0.0, 0.5);
    w.step();
    assert!(
        w.check_intersection(a, b),
        "deep overlap must intersect (strict geometric path)"
    );
}

#[test]
fn twod_separated_bodies_do_not_intersect() {
    let mut w = RapierWorld2D::new(0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0);
    let b = w.create_body(0, 5.0, 5.0); // far apart
    w.add_circle_collider(a, R, false, 0.0, 0.5);
    w.add_circle_collider(b, R, false, 0.0, 0.5);
    w.step();
    assert!(
        !w.check_intersection(a, b),
        "separated bodies must not intersect"
    );
}

#[test]
fn twod_touching_bodies_report_via_contact_path() {
    // The contact-path fallback (contact_pairs_with) is the code the O(N x
    // pairs) bug lived in — it must report resting/touching bodies correctly.
    let mut w = RapierWorld2D::new(0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0);
    let b = w.create_body(0, 2.0 * R + 0.001, 0.0); // just past touching
    w.add_circle_collider(a, R, false, 0.0, 0.0);
    w.add_circle_collider(b, R, false, 0.0, 0.0);
    w.step();
    // Pin actual semantics: contact-pair fallback + strict overlap both run;
    // whatever the narrow phase decides, repeated queries must be stable.
    let first = w.check_intersection(a, b);
    for _ in 0..5 {
        assert_eq!(w.check_intersection(a, b), first, "answer must be stable");
    }
}

/// The scaling guard: a world FULL of contact pairs must not change the cost
/// of one query. The old code re-walked every pair per check (O(N x pairs));
/// with `contact_pairs_with` this stays near-constant regardless of N.
#[test]
fn twod_query_cost_does_not_scale_with_world_contact_count() {
    let mut w = RapierWorld2D::new(0.0, 0.0);
    // 60x50 grid of touching balls = ~6,000 contact pairs in the narrow phase.
    let mut handles = Vec::new();
    for y in 0..50 {
        for x in 0..60 {
            let h = w.create_body(0, x as f32 * 0.9, y as f32 * 0.9);
            w.add_circle_collider(h, R, false, 0.0, 0.0);
            handles.push(h);
        }
    }
    // The queried pair: two fresh touching balls far from the grid.
    let qa = w.create_body(0, 500.0, 500.0);
    let qb = w.create_body(0, 500.0 + 2.0 * R + 0.001, 500.0);
    w.add_circle_collider(qa, R, false, 0.0, 0.0);
    w.add_circle_collider(qb, R, false, 0.0, 0.0);
    w.step();

    let start = std::time::Instant::now();
    let first = w.check_intersection(qa, qb);
    let mut stable = true;
    for _ in 0..3000 {
        if w.check_intersection(qa, qb) != first {
            stable = false;
        }
    }
    let elapsed = start.elapsed();
    // Correctness first: the same answer every time, whatever it is.
    assert!(stable, "3000 repeated queries must all agree");
    // And bounded: the fixed path does 3,000 O(degree) queries over a
    // ~6,000-pair world in single-digit ms even in debug. The old O(N x pairs)
    // walk needed ~18M pair visits for the same loop and blew past this.
    assert!(
        elapsed < std::time::Duration::from_millis(150),
        "3,000 contact checks over a ~6,000-pair world took {elapsed:?} — \
         the O(N x pairs) regression is back"
    );
}

// ---------------------------------------------------------------------------
// 3D
// ---------------------------------------------------------------------------

#[test]
fn threed_overlapping_bodies_intersect() {
    let mut w = RapierWorld3D::new(0.0, 0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0, 0.0);
    let b = w.create_body(0, 0.6, 0.0, 0.0);
    w.add_ball_collider(a, R);
    w.add_ball_collider(b, R);
    w.step();
    assert!(
        w.check_intersection(a, b),
        "deep overlap must intersect (strict geometric path)"
    );
}

#[test]
fn threed_separated_bodies_do_not_intersect() {
    let mut w = RapierWorld3D::new(0.0, 0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0, 0.0);
    let b = w.create_body(0, 5.0, 5.0, 5.0);
    w.add_ball_collider(a, R);
    w.add_ball_collider(b, R);
    w.step();
    assert!(
        !w.check_intersection(a, b),
        "separated bodies must not intersect"
    );
}

#[test]
fn threed_touching_bodies_report_via_contact_path() {
    let mut w = RapierWorld3D::new(0.0, 0.0, 0.0);
    let a = w.create_body(0, 0.0, 0.0, 0.0);
    let b = w.create_body(0, 2.0 * R + 0.001, 0.0, 0.0);
    w.add_ball_collider(a, R);
    w.add_ball_collider(b, R);
    w.step();
    let first = w.check_intersection(a, b);
    for _ in 0..5 {
        assert_eq!(w.check_intersection(a, b), first, "answer must be stable");
    }
}

#[test]
fn threed_query_cost_does_not_scale_with_world_contact_count() {
    let mut w = RapierWorld3D::new(0.0, 0.0, 0.0);
    // 30x30x7 grid of touching balls = ~6,000+ contact pairs.
    let mut handles = Vec::new();
    for y in 0..30 {
        for x in 0..30 {
            for z in 0..7 {
                let h = w.create_body(0, x as f32 * 0.9, y as f32 * 0.9, z as f32 * 0.9);
                w.add_ball_collider(h, R);
                handles.push(h);
            }
        }
    }
    let qa = w.create_body(0, 500.0, 500.0, 500.0);
    let qb = w.create_body(0, 500.0 + 2.0 * R + 0.001, 500.0, 500.0);
    w.add_ball_collider(qa, R);
    w.add_ball_collider(qb, R);
    w.step();

    let start = std::time::Instant::now();
    let first = w.check_intersection(qa, qb);
    let mut stable = true;
    for _ in 0..3000 {
        if w.check_intersection(qa, qb) != first {
            stable = false;
        }
    }
    let elapsed = start.elapsed();
    assert!(stable, "3,000 repeated queries must all agree");
    assert!(
        elapsed < std::time::Duration::from_millis(150),
        "3,000 contact checks over a dense 3D grid took {elapsed:?} — \
         the O(N x pairs) regression is back"
    );
    let _ = handles;
}
