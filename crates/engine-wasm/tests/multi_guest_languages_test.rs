use engine_wasm::{WasmConfig, WasmRuntime};
use wasmtime::{Caller, Linker};

fn setup_linker<T: 'static>(runtime: &WasmRuntime) -> Linker<T> {
    let mut linker = runtime.create_linker::<T>();

    // AssemblyScript and freestanding C runtimes may import env::abort
    linker
        .func_wrap(
            "env",
            "abort",
            |_caller: Caller<'_, T>,
             msg_ptr: i32,
             file_ptr: i32,
             line: i32,
             col: i32| {
                eprintln!(
                    "[guest abort] msg_ptr={}, file_ptr={}, line={}, col={}",
                    msg_ptr, file_ptr, line, col
                );
            },
        )
        .expect("linker abort func_wrap ok");

    // Standard OpenOxide Host Logging ABI
    linker
        .func_wrap(
            "env",
            "host_log",
            |_caller: Caller<'_, T>, _ptr: i32, _len: i32| -> i32 { 0 },
        )
        .expect("linker host_log func_wrap ok");

    linker
}

fn test_guest_wasm_lifecycle(wasm_bytes: &[u8], guest_name: &str, expected_magic: u8) {
    println!("\n=======================================================");
    println!("  TESTING GUEST WASM LIFECYCLE: {}", guest_name);
    println!("=======================================================");
    println!("Binary size: {} bytes", wasm_bytes.len());

    let config = WasmConfig {
        optimize_for_speed: true,
        enable_epoch_interruption: false,
        use_pooling_allocator: false,
    };
    let runtime = WasmRuntime::new_with_config(config).expect("Runtime creation ok");
    let module = runtime.compile(wasm_bytes).expect("WASM compilation ok");

    let linker = setup_linker::<()>(&runtime);
    let mut instance = runtime
        .instantiate(&linker, &module, ())
        .expect("WASM instantiation ok");

    // 1. Resolve and verify all 9 canonical exported functions
    let on_create = instance
        .get_typed_func::<i32, ()>("on_create")
        .expect("on_create must be exported");
    let on_is_full = instance
        .get_typed_func::<i32, i32>("on_is_full")
        .expect("on_is_full must be exported");
    let on_join = instance
        .get_typed_func::<(i64, f64, f64), ()>("on_join")
        .expect("on_join must be exported");
    let on_alloc = instance
        .get_typed_func::<i32, i32>("on_alloc")
        .expect("on_alloc must be exported");
    let on_input = instance
        .get_typed_func::<(i64, i32, i32), ()>("on_input")
        .expect("on_input must be exported");
    let on_tick = instance
        .get_typed_func::<f64, i64>("on_tick")
        .expect("on_tick must be exported");
    let on_save = instance
        .get_typed_func::<(), i64>("on_save")
        .expect("on_save must be exported");
    let on_leave = instance
        .get_typed_func::<i64, ()>("on_leave")
        .expect("on_leave must be exported");
    let on_load = instance
        .get_typed_func::<(i32, i32), ()>("on_load")
        .expect("on_load must be exported");

    println!("  [1/9] All 9 canonical lifecycle hooks resolved successfully");

    // 2. on_create: Initialize room with max 32 players
    on_create
        .call(instance.store_mut(), 32)
        .expect("on_create call ok");
    println!("  [2/9] on_create(32) passed");

    // 3. on_is_full: Validate capacity check
    let full_at_10 = on_is_full
        .call(instance.store_mut(), 10)
        .expect("on_is_full(10) ok");
    assert_eq!(full_at_10, 0, "Room should not be full at 10/32");

    let full_at_32 = on_is_full
        .call(instance.store_mut(), 32)
        .expect("on_is_full(32) ok");
    assert_ne!(full_at_32, 0, "Room should be full at 32/32");
    println!("  [3/9] on_is_full capacity assertions passed");

    // 4. on_join: Spawn player 1001
    on_join
        .call(instance.store_mut(), (1001, 10.0, 20.0))
        .expect("on_join call ok");
    println!("  [4/9] on_join(id=1001, x=10.0, z=20.0) passed");

    // 5. on_alloc + write_memory: Allocate buffer in guest linear memory
    let input_bytes = [0x01, 0x00, 0x00, 0x00, 0x05, 0x05, 0x00, 0x00];
    let alloc_ptr = on_alloc
        .call(instance.store_mut(), input_bytes.len() as i32)
        .expect("on_alloc call ok");
    assert!(alloc_ptr > 0, "Allocated pointer must be non-zero");

    instance
        .write_memory(alloc_ptr as u32, &input_bytes)
        .expect("Writing packet to guest linear memory ok");
    println!(
        "  [5/9] on_alloc({} bytes) -> ptr=0x{:X} passed",
        input_bytes.len(),
        alloc_ptr
    );

    // 6. on_input: Dispatch packet to player 1001
    on_input
        .call(
            instance.store_mut(),
            (1001, alloc_ptr, input_bytes.len() as i32),
        )
        .expect("on_input call ok");
    println!("  [6/9] on_input dispatched packet to player 1001");

    // 7. on_tick: Authoritative simulation step (dt = 0.016s -> 60 Hz)
    let packed_delta = on_tick
        .call(instance.store_mut(), 0.016)
        .expect("on_tick call ok");
    let delta_ptr = ((packed_delta as u64) >> 32) as u32;
    let delta_len = (packed_delta as u64 & 0xFFFFFFFF) as u32;

    assert!(delta_ptr > 0, "Delta pointer must be non-zero");
    assert!(delta_len > 0, "Delta length must be greater than zero");

    let delta_data = instance
        .read_memory(delta_ptr, delta_len)
        .expect("Reading delta buffer from linear memory ok");
    assert!(
        delta_data.len() >= 2,
        "Delta buffer must contain at least 2 bytes"
    );
    assert_eq!(
        delta_data[0], 1,
        "Delta buffer byte 0 must record 1 active player"
    );
    assert_eq!(
        delta_data[1], expected_magic,
        "Delta buffer magic mismatch for guest language!"
    );
    println!(
        "  [7/9] on_tick(0.016) -> delta=0x{:X} len={} (magic 0x{:02X} verified)",
        delta_ptr, delta_len, delta_data[1]
    );

    // 8. on_save & on_leave: Snapshot room state and disconnect player
    let packed_save = on_save.call(instance.store_mut(), ()).expect("on_save ok");
    let save_ptr = ((packed_save as u64) >> 32) as u32;
    let save_len = (packed_save as u64 & 0xFFFFFFFF) as u32;

    assert!(save_ptr > 0, "Save pointer must be non-zero");
    assert!(save_len > 0, "Save length must be greater than zero");
    let _saved_snapshot = instance
        .read_memory(save_ptr, save_len)
        .expect("Reading save snapshot ok");

    on_leave
        .call(instance.store_mut(), 1001)
        .expect("on_leave call ok");
    println!(
        "  [8/9] on_save(len={}) and on_leave(1001) passed",
        save_len
    );

    // Verify after leave: player count becomes 0
    let packed_delta_after_leave = on_tick.call(instance.store_mut(), 0.016).expect("on_tick ok");
    let p_ptr = ((packed_delta_after_leave as u64) >> 32) as u32;
    let p_data = instance
        .read_memory(p_ptr, 1)
        .expect("Reading updated delta ok");
    assert_eq!(p_data[0], 0, "Player count must be 0 after player left");

    // 9. on_load: Restore state from snapshot
    on_load
        .call(instance.store_mut(), (save_ptr as i32, save_len as i32))
        .expect("on_load call ok");

    let packed_delta_after_restore = on_tick.call(instance.store_mut(), 0.016).expect("on_tick ok");
    let r_ptr = ((packed_delta_after_restore as u64) >> 32) as u32;
    let r_data = instance
        .read_memory(r_ptr, 1)
        .expect("Reading restored delta ok");
    assert_eq!(r_data[0], 1, "Player count must be restored to 1 from saved snapshot");
    println!("  [9/9] on_load restored snapshot successfully (verified player_count=1)");

    println!(
        "  >>> SUCCESS: All 9 canonical lifecycle methods verified for {}! <<<\n",
        guest_name
    );
}

#[test]
fn test_all_guest_languages_comprehensive() {
    // 1. Pure C Guest WASM
    let c_wasm = include_bytes!("../../../tests/guest_c/room.wasm");
    test_guest_wasm_lifecycle(c_wasm, "Pure C (Clang Freestanding)", 0xAA);

    // 2. C++ Guest WASM
    let cpp_wasm = include_bytes!("../../../tests/guest_cpp/room.wasm");
    test_guest_wasm_lifecycle(cpp_wasm, "C++ (Clang++ Freestanding)", 0xCC);

    // 3. AssemblyScript Guest WASM
    let as_wasm = include_bytes!("../../../tests/guest_as/room.wasm");
    test_guest_wasm_lifecycle(as_wasm, "AssemblyScript (asc -Ospeed --runtime stub)", 0x55);

    // 4. Rust Guest WASM
    let rust_wasm = include_bytes!("../../../tests/guest_rust/room.wasm");
    test_guest_wasm_lifecycle(rust_wasm, "Rust (wasm32-unknown-unknown)", 0xEE);

    // 5. Lua Guest (Native Rust embedded LuaJIT via mlua)
    println!("=======================================================");
    println!("  TESTING GUEST RUNTIME: Lua (Native mlua & LuaJIT)");
    println!("=======================================================");
    let lua_script = include_str!("../../../tests/guest_lua/room.lua");
    let lua_room = engine_lua::LuaRoom::from_source(lua_script)
        .expect("Initializing native LuaRoom via mlua ok");

    // 1. onCreate
    lua_room.on_create(32).expect("Lua onCreate ok");
    println!("  [1/8] Lua onCreate(32) passed");

    // 2. onIsFull
    assert!(!lua_room.on_is_full(10).expect("on_is_full ok"));
    assert!(lua_room.on_is_full(32).expect("on_is_full ok"));
    println!("  [2/8] Lua onIsFull capacity assertions passed");

    // 3. onJoin
    lua_room.on_join(1001, 10.0, 20.0).expect("Lua onJoin ok");
    println!("  [3/8] Lua onJoin(1001) passed");

    // 4. onInput
    let packet = [0x01, 0x02, 0x03, 0x04];
    lua_room.on_input(1001, &packet).expect("Lua onInput ok");
    println!("  [4/8] Lua onInput packet dispatch passed");

    // 5. onTick
    let delta = lua_room.on_tick(0.016).expect("Lua onTick ok");
    assert_eq!(delta.len(), 3);
    assert_eq!(delta[0], 1, "Player count must be 1");
    assert_eq!(delta[1], 0x77, "Lua magic signature 0x77 verified");
    println!("  [5/8] Lua onTick(0.016) -> len=3 (magic 0x77 verified)");

    // 6. onSave & onLeave
    let snapshot = lua_room.on_save().expect("Lua onSave ok");
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0], 0x33);
    lua_room.on_leave(1001).expect("Lua onLeave ok");
    let delta_after_leave = lua_room.on_tick(0.016).expect("onTick after leave ok");
    assert_eq!(delta_after_leave[0], 0, "Player count must be 0 after leave");
    println!("  [6/8] Lua onSave(len=2) and onLeave(1001) passed");

    // 7. onLoad
    lua_room.on_load(&snapshot).expect("Lua onLoad ok");
    let delta_after_restore = lua_room.on_tick(0.016).expect("onTick after restore ok");
    assert_eq!(delta_after_restore[0], 1, "Player count must be restored to 1");
    println!("  [7/8] Lua onLoad state restore verified (player_count=1)");

    println!("  >>> SUCCESS: All native LuaRoom hooks verified via mlua! <<<\n");
}
