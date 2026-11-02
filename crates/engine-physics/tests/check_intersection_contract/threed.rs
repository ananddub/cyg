use crate::R;
use engine_physics::RapierWorld3D;

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
