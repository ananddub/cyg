//! Unit tests for the @colyseus/schema-compatible state encoder.

use engine_net::colyseus_state::ColyseusRoomState;

#[test]
fn test_snapshot_wire_format() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 1.5, -2.0);

    let snap = state.snapshot_consuming();

    // Root: players field ADD (0x80|0) + msgpack refId 1
    assert_eq!(snap[0], 0x80, "root players field ADD");
    assert_eq!(snap[1], 0x01, "players map refId = 1");
    // Switch to map
    assert_eq!(&snap[2..4], &[0xFF, 0x01]);
    // Map entry ADD: op, index, fixstr "p1", player refId 2
    assert_eq!(snap[4], 0x80, "map entry ADD op");
    assert_eq!(snap[5], 0x01, "map index 1");
    assert_eq!(snap[6], 0xA2, "fixstr len 2");
    assert_eq!(&snap[7..9], b"p1");
    assert_eq!(snap[9], 0x02, "player refId = 2");
    // Switch to player
    assert_eq!(&snap[10..12], &[0xFF, 0x02]);
    // x (field 0, ADD) = msgpack float64 1.5 (0xCB + LE bytes)
    assert_eq!(snap[12], 0x80);
    assert_eq!(snap[13], 0xCB);
    assert_eq!(&snap[14..22], &1.5f64.to_le_bytes());
    // y (field 1, ADD) = -2.0 integral -> msgpack negative fixint (0xe0 | (v+32))
    assert_eq!(snap[22], 0x81);
    assert_eq!(snap[23], 0xFE, "-2.0 as negative fixint");
    // Back to root, tick (field 1, ADD) = 0
    assert_eq!(&snap[24..26], &[0xFF, 0x00]);
    assert_eq!(snap[26], 0x81);
    assert_eq!(snap[27], 0x00, "tick 0 = msgpack fixint 0");
    assert_eq!(snap.len(), 28);
}

#[test]
fn test_patch_replace_and_new_player() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 1.0, 2.0);
    let _snapshot = state.snapshot_consuming(); // clear initial dirty flags

    state.advance_tick();
    state.update_player("p1", 3.5, 4.5);

    let patch = state.patch();

    // Switch to player ref 2, REPLACE x/y (op byte = bare field index)
    assert_eq!(&patch[0..2], &[0xFF, 0x02]);
    assert_eq!(patch[2], 0x00, "x REPLACE (field index 0, op REPLACE=0)");
    assert_eq!(patch[3], 0xCB);
    assert_eq!(&patch[4..12], &3.5f64.to_le_bytes());
    assert_eq!(patch[12], 0x01, "y REPLACE");
    assert_eq!(patch[13], 0xCB);
    assert_eq!(&patch[14..22], &4.5f64.to_le_bytes());
    // Back to root, tick REPLACE = 1
    assert_eq!(&patch[22..24], &[0xFF, 0x00]);
    assert_eq!(patch[24], 0x01);
    assert_eq!(patch[25], 0x01, "tick 1 = msgpack fixint 1");
}

#[test]
fn test_patch_includes_new_player_add() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 0.0, 0.0);
    let _ = state.snapshot_consuming();

    // Use fractional values so fields encode as float64 (stable byte layout).
    state.add_player("p2", 5.5, 6.25);
    let patch = state.patch();

    // Switch to map (ref 1), ADD entry for p2 (index 2, refId 3)
    assert_eq!(&patch[0..2], &[0xFF, 0x01]);
    assert_eq!(patch[2], 0x80, "map entry ADD");
    assert_eq!(patch[3], 0x02, "map index 2");
    assert_eq!(patch[4], 0xA2, "fixstr len 2");
    assert_eq!(&patch[5..7], b"p2");
    assert_eq!(patch[7], 0x03, "p2 refId = 3");
    // Switch to p2 (ref 3), REPLACE fields
    assert_eq!(&patch[8..10], &[0xFF, 0x03]);
    assert_eq!(patch[10], 0x00);
    assert_eq!(patch[11], 0xCB);
    assert_eq!(&patch[12..20], &5.5f64.to_le_bytes());
    assert_eq!(patch[20], 0x01);
    assert_eq!(patch[21], 0xCB);
    assert_eq!(&patch[22..30], &6.25f64.to_le_bytes());
}

#[test]
fn test_empty_patch_when_nothing_changed() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 0.0, 0.0);
    let _ = state.snapshot_consuming();
    let patch = state.patch();
    assert!(patch.is_empty(), "no changes -> empty patch");
}

#[test]
fn test_msgpack_number_encodings() {
    // Verify via public API: snapshot tick values exercise fixint paths.
    let mut state = ColyseusRoomState::new();
    state.advance_tick(); // tick = 1
    state.add_player("a", 1000000.0, -0.25);
    let snap = state.snapshot();

    // tick: uint16 encoding (0xcd) little-endian for 1000000.0? No — x is float.
    // x = 1000000.0 is integral -> msgpack uint32 (0xce) LE.
    let x_pos = snap
        .windows(5)
        .position(|w| w == [0x80, 0xce, 0x40, 0x42, 0x0f]);
    assert!(x_pos.is_some(), "1000000.0 as uint32 LE after ADD op");

    // y = -0.25 is fractional -> float64 (0xcb) LE.
    let y_pos = snap.windows(9).position(|w| w[0] == 0x81 && w[1] == 0xcb);
    assert!(y_pos.is_some(), "-0.25 as float64");
}
