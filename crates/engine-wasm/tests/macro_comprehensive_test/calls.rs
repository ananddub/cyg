use super::fixture::open_room;

#[test]
fn test_macro_calls_host_memory_and_trap() {
    let mut room = open_room();

    room.reset().expect("reset call ok");
    assert_eq!(
        room.data().events_count,
        1,
        "Host state should be updated via host import"
    );

    room.set_speed(250.5).expect("set_speed ok");

    let spawned_id = room
        .spawn_entity(1001, 12.5, 45.0, 2)
        .expect("spawn_entity ok");
    assert_eq!(spawned_id, 1003);

    let payload = b"Hello from Macro Memory API";
    room.write_memory(64, payload).expect("write_memory ok");
    let read_back = room
        .read_memory(64, payload.len() as u32)
        .expect("read_memory ok");
    assert_eq!(read_back, payload);

    let trap_result = room.cause_trap();
    assert!(trap_result.is_err(), "cause_trap must return an Err");
}
