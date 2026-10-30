use crate::infra::seats::*;

fn registry(previous: &str) -> SeatRegistry {
    SeatRegistry::with_previous("new-secret", previous, MAX_OUTSTANDING_SEATS)
}

#[test]
fn a_seat_minted_after_the_rotation_verifies_on_both_sides_of_the_window() {
    let mut r = registry("old-secret");
    let seat = r.mint("tank-3d", 300).unwrap();
    assert!(registry("old-secret").is_valid(&seat.token));
    assert!(registry("").is_valid(&seat.token));
}

#[test]
fn a_seat_signed_with_the_retired_secret_is_trusted_only_while_the_window_is_open() {
    // The registry as it stood before the rotation: "old-secret" was its current.
    let before = SeatRegistry::with_previous("old-secret", "", MAX_OUTSTANDING_SEATS);
    let retired = before.sign("tank-3d", 4_000_000_000);
    assert!(
        registry("old-secret").is_valid(&retired),
        "seats handed out before the rotation must survive it"
    );
    assert!(
        !registry("").is_valid(&retired),
        "once the retired secret is dropped the seat must stop verifying"
    );
}

#[test]
fn a_secret_that_is_not_the_current_or_retired_one_verifies_nothing() {
    let foreign = SeatRegistry::new("unrelated-secret").sign("tank-3d", 4_000_000_000);
    assert!(!registry("old-secret").is_valid(&foreign));
}
