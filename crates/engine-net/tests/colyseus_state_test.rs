//! Wire-format regression tests for the schema state encoder.
//! Byte layout verified against @colyseus/schema (0.18 monorepo fork).

use engine_net::colyseus_state::ColyseusRoomState;

#[test]
fn test_snapshot_wire_format() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 1.5, -2.0, 0.0);

    let snap = state.snapshot();

    // Layout:
    // [80 01]                       root players ADD, map refId 1
    // [FF 01]                       switch to map
    // [80 01 A2 70 31 02]           map entry ADD: idx 1, key "p1", refId 2
    // [FF 02]                       switch to player
    // [80 CB <f64 1.5>]             x ADD (field 0)
    // [81 FE]                       y ADD (field 1, -2.0 fixint)
    // [82 00]                       score ADD (field 2, fixint 0)
    // [FF 00]                       switch to root
    // [82 00 83 00 81 00]           coin_x / coin_y / tick ADD (all 0)
    assert_eq!(snap[0], 0x80, "root players field ADD");
    assert_eq!(snap[1], 0x01, "players map refId = 1");
    assert_eq!(&snap[2..4], &[0xFF, 0x01]);
    assert_eq!(snap[4], 0x80, "map entry ADD op");
    assert_eq!(snap[5], 0x01, "map index 1");
    assert_eq!(snap[6], 0xA2, "fixstr len 2");
    assert_eq!(&snap[7..9], b"p1");
    assert_eq!(snap[9], 0x02, "player refId = 2");
    assert_eq!(&snap[10..12], &[0xFF, 0x02]);
    assert_eq!(snap[12], 0x80);
    assert_eq!(snap[13], 0xCB);
    assert_eq!(&snap[14..22], &1.5f64.to_le_bytes());
    assert_eq!(snap[22], 0x81);
    assert_eq!(snap[23], 0xFE, "-2.0 as negative fixint");
    assert_eq!(snap[24], 0x82, "score ADD (field 2)");
    assert_eq!(snap[25], 0x00, "score = 0");
    assert_eq!(&snap[26..28], &[0xFF, 0x00], "switch to root");
    assert_eq!(snap[28], 0x82, "coin_x ADD (field 2)");
    assert_eq!(snap[29], 0x00, "coin at origin");
    assert_eq!(snap[30], 0x83, "coin_y ADD (field 3)");
    assert_eq!(snap[31], 0x00);
    assert_eq!(snap[32], 0x81, "tick ADD (field 1)");
    assert_eq!(snap[33], 0x00, "tick = 0");
    assert_eq!(snap.len(), 34);
}

#[test]
fn test_patch_replace_and_new_player() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 1.0, 2.0, 0.0);
    let _snapshot = state.snapshot_consuming(); // clear initial dirty flags

    state.set_coin(100.0, 200.0);
    state.advance_tick();
    state.update_player("p1", 3.5, 4.5, 0.0);

    let patch = state.patch();

    // Layout:
    // [FF 02]                       switch to player
    // [00 CB <f64 3.5>]             x REPLACE
    // [01 CB <f64 4.5>]             y REPLACE
    // [02 00]                       score REPLACE
    // [FF 00]                       switch to root
    // [02 CD 90 01]                 coin_x REPLACE (uint16 LE 100)
    // [03 CC C8]                    coin_y REPLACE (uint8 200)
    // [01 01]                       tick REPLACE
    assert_eq!(&patch[0..2], &[0xFF, 0x02]);
    assert_eq!(patch[2], 0x00, "x REPLACE (field 0, op REPLACE=0)");
    assert_eq!(patch[3], 0xCB);
    assert_eq!(&patch[4..12], &3.5f64.to_le_bytes());
    assert_eq!(patch[12], 0x01, "y REPLACE");
    assert_eq!(&patch[14..22], &4.5f64.to_le_bytes());
    assert_eq!(patch[22], 0x02, "score REPLACE");
    assert_eq!(patch[23], 0x00, "score = 0");
    assert_eq!(&patch[24..26], &[0xFF, 0x00]);
    assert_eq!(patch[26], 0x02, "coin_x REPLACE (fixint 100)");
    assert_eq!(patch[27], 0x64);
    assert_eq!(patch[28], 0x03, "coin_y REPLACE");
    assert_eq!(&patch[29..31], &[0xcc, 0xc8], "uint16 200");
    assert_eq!(patch[31], 0x01);
    assert_eq!(patch[32], 0x01, "tick 1 = msgpack fixint 1");
}

#[test]
fn test_patch_includes_new_player_add() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 0.0, 0.0, 0.0);
    let _ = state.snapshot_consuming();

    // Use fractional values so fields encode as float64 (stable byte layout).
    state.add_player("p2", 5.5, 6.25, 0.0);
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
    state.add_player("p1", 0.0, 0.0, 0.0);
    let _ = state.snapshot_consuming();
    let patch = state.patch();
    assert!(patch.is_empty(), "no changes -> empty patch");
}

#[test]
fn test_msgpack_number_encodings() {
    let mut state = ColyseusRoomState::new();
    state.advance_tick(); // tick = 1
    state.add_player("a", 1000000.0, -0.25, 0.0);
    let snap = state.snapshot();

    // x = 1000000.0 is integral -> msgpack uint32 (0xce) little-endian.
    let x_pos = snap
        .windows(5)
        .position(|w| w == [0x80, 0xce, 0x40, 0x42, 0x0f]);
    assert!(x_pos.is_some(), "1000000.0 as uint32 LE after ADD op");

    // y = -0.25 is fractional -> float64 (0xcb) LE.
    let y_pos = snap.windows(9).position(|w| w[0] == 0x81 && w[1] == 0xcb);
    assert!(y_pos.is_some(), "-0.25 as float64");
}

#[test]
fn test_remove_player_emits_delete() {
    let mut state = ColyseusRoomState::new();
    state.add_player("p1", 1.0, 2.0, 0.0);
    let _ = state.snapshot_consuming();

    state.remove_player("p1");
    let patch = state.patch();

    // [FF 01][40 01] switch to map + DELETE map index 1
    assert_eq!(&patch[0..4], &[0xFF, 0x01, 0x40, 0x01]);
}

#[test]
fn test_reconnection_seat_reuses_player_key() {
    // Disconnect then re-join with the same session: the schema state must
    // treat it as a brand-new player (fresh ADD), not a resurrect.
    let mut state = ColyseusRoomState::new();
    state.add_player("sess_A", 10.0, 20.0, 0.0);
    let _ = state.snapshot_consuming();
    state.remove_player("sess_A");
    // The DELETE is emitted exactly once, in the patch right after removal.
    let del_patch = state.patch();
    assert_eq!(&del_patch[0..4], &[0xFF, 0x01, 0x40, 0x01], "switch + DELETE old map index 1");
    state.add_player("sess_A", 30.0, 40.0, 5.0);
    let patch = state.patch();
    // Fresh ADD only — the consumed DELETE must not resurrect in this patch:
    // [FF 01] switch to map, [80 02] entry ADD idx 2, [A6 ...] key, [03] refId.
    assert_eq!(&patch[0..2], &[0xFF, 0x01]);
    assert_eq!(patch[2], 0x80, "map entry ADD");
    assert_eq!(patch[3], 0x02, "fresh map index");
    assert_eq!(patch[4], 0xa6, "fixstr key len 6");
    assert_eq!(&patch[5..11], b"sess_A");
    assert_eq!(patch[11], 0x03, "fresh refId 3");
}
