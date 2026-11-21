use crate::infra::seats::*;
use engine_config::DEV_SEAT_SECRET;

#[test]
fn a_fixed_vector_pins_the_token_bytes() {
    let r = SeatRegistry::new(DEV_SEAT_SECRET);
    assert_eq!(
        r.sign("tank-3d", 1_700_000_000),
        "dGFuay0zZDoxNzAwMDAwMDAw._eStiNqMvk96oc6AF2bBoi50UqL24Q67IkJjBl4SvBQ"
    );
    assert_eq!(
        r.sign("arena_final", 1_800_000_000),
        "YXJlbmFfZmluYWw6MTgwMDAwMDAwMA.36cihDFBJE7OU6Rd32jVH063Q5lji7WDXbvt6AaxFcY"
    );
}
