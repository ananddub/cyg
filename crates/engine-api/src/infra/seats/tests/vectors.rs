use crate::infra::seats::*;
use engine_config::DEV_SEAT_SECRET;

#[test]
fn a_fixed_vector_pins_the_token_bytes() {
    let r = SeatRegistry::new(DEV_SEAT_SECRET);
    assert_eq!(
        r.sign("tank-3d", 1_700_000_000),
        "dGFuay0zZDoxNzAwMDAwMDAw.4y2xJ1Nc7shWlds8ZcQPA_ntwg3irdni9ltMKvRIOkE"
    );
    assert_eq!(
        r.sign("arena_final", 1_800_000_000),
        "YXJlbmFfZmluYWw6MTgwMDAwMDAwMA.o5CjkR7hxLmyPiL_M6HR6p-4HY3oXjn-x_lHMyb1zfE"
    );
}
