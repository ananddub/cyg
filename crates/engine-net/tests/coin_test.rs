use engine_net::colyseus_state::ColyseusRoomState;

#[test]
fn debug_coin_patch() {
    let mut state = ColyseusRoomState::new();
    state.add_player("s1", 60.0, 60.0, 0.0);
    let snap = state.snapshot();
    println!("snapshot ({}B): {:02X?}", snap.len(), snap);
    println!("coin fields in snapshot: 82={:?} 83={:?}", 
        snap.windows(2).position(|w| w == [0x82, 0x00]),
        snap.windows(2).position(|w| w == [0x83, 0x00]));

    state.set_coin(400.0, 250.0);
    state.advance_tick();
    let patch = state.patch();
    println!("patch ({}B): {:02X?}", patch.len(), patch);
    let has_coin = patch.windows(1).any(|w| w == [0x02]) && patch.windows(2).any(|w| w == [0x02, 0x00]);
    println!("coin REPLACE present: {has_coin}");
}
