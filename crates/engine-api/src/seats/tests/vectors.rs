use crate::seats::*;
use engine_config::DEV_SEAT_SECRET;

// The raw-UDP plane (`src/threads/seats.rs`, engine-server) is the verifier
// that authenticates `JOIN [0x20][token_len u8][token]` on :7778. Its
// bidirectional cross-plane tests cannot live here (engine-control must not
// depend on engine-server); tracked as an integration TODO. The byte layout
// itself is pinned on this side by the fixed vectors below.

#[test]
fn a_fixed_vector_pins_the_token_bytes() {
    // Computed independently (OpenSSL/Node semantics, no Rust involved):
    //   payload = "tank-3d:1700000000"
    //   mac     = HMAC-SHA256("ox-dev-seat-secret", payload bytes)
    //   token   = b64url(payload) + "." + b64url(mac), unpadded
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
