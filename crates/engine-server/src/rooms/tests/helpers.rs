use super::*;

    const LIFECYCLE_GUEST_WAT: &str = r#"
        (module

          (memory (export "memory") 1)

          (global $ticks (mut i32) (i32.const 0))

          (global $leaves (mut i32) (i32.const 0))

          (global $reconnects (mut i32) (i32.const 0))

          (global $disposals (mut i32) (i32.const 0))
          (global $heap (mut i32) (i32.const 1024))

          (func (export "on_create") (param i32 i32 i32))

          (func (export "on_join") (param i64 i32 i32 i32 i32))

          (func (export "on_leave") (param i64 i32)

            (global.set $leaves (i32.add (global.get $leaves) (i32.const 1))))

          (func (export "on_alloc") (param i32) (result i32)
            (local $at i32)
            (local.set $at (global.get $heap))
            (global.set $heap (i32.add (global.get $heap) (local.get 0)))
            (local.get $at))

          (func (export "on_input") (param i64 i32 i32))

          (func (export "on_tick") (param f64) (result i64)

            (global.set $ticks (i32.add (global.get $ticks) (i32.const 1)))

            (i64.const 0))

          (func (export "on_save") (result i64) (i64.const 0))

          (func (export "on_load") (param i32 i32))

          (func (export "on_auth") (param i64 i32 i32) (result i32)

            (i32.and

              (i32.gt_s (local.get 2) (i32.const 0))

              (i32.eq (i32.load8_u (local.get 1)) (i32.const 123))))

          (func (export "on_allow_reconnection") (param i64) (result i32) (i32.const 5000))

          (func (export "on_reconnect") (param i64)

            (global.set $reconnects (i32.add (global.get $reconnects) (i32.const 1))))

          (func (export "on_dispose")

            (global.set $disposals (i32.add (global.get $disposals) (i32.const 1))))

          (func (export "on_patch_rate") (result i32) (i32.const 100))

          (global $patch_due_state (mut i32) (i32.const 1))

          (func (export "on_patch_due") (param i32)

            (global.set $patch_due_state (local.get 0)))

          (func (export "patch_due_state") (result i32) (global.get $patch_due_state))

          (func (export "on_simulation_rate") (result i32) (i32.const 50))

          (func (export "tick_count") (result i32) (global.get $ticks))

          (func (export "leave_count") (result i32) (global.get $leaves))

          (func (export "reconnect_count") (result i32) (global.get $reconnects))

          (func (export "dispose_count") (result i32) (global.get $disposals))

        )

    "#;

    /// A guest whose `on_input` kicks the sender via `host_kick` and whose
    /// `on_leave` records the close code it received.
    pub(crate) fn kick_room() -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let wat = r#"
            (module
              (import "env" "host_kick" (func $kick (param i64) (result i32)))
              (memory (export "memory") 1)
              (global $last_code (mut i32) (i32.const 0))
              (func (export "on_create") (param i32 i32 i32))
              (func (export "on_join") (param i64 i32 i32 i32 i32))
              (func (export "on_leave") (param i64 i32)
                (global.set $last_code (local.get 1)))
              (func (export "on_alloc") (param i32) (result i32) (i32.const 0))
              (func (export "on_input") (param i64 i32 i32)
                (drop (call $kick (local.get 0))))
              (func (export "on_tick") (param f64) (result i64) (i64.const 0))
              (func (export "on_save") (result i64) (i64.const 0))
              (func (export "on_load") (param i32 i32))
              (func (export "last_code") (result i32) (global.get $last_code)))
        "#;
        let module = runtime.compile(wat.as_bytes()).expect("compile kick wat");
        let linker = build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate kick room")
    }

    pub(crate) fn lifecycle_room() -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let module = runtime
            .compile(LIFECYCLE_GUEST_WAT.as_bytes())
            .expect("compile lifecycle wat");
        let linker = build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate lifecycle room")
    }

    pub(crate) fn guest_i32(room: &mut GameRoom, export: &str) -> i32 {
        room.wasm
            .as_mut()
            .expect("wasm guest")
            .raw_instance()
            .call::<(), i32>(export, ())
            .expect("counter export")
    }

    pub(crate) const ABI_GUEST_WAT: &str = r#"
        (module

          (memory (export "memory") 1)

          (func (export "on_create") (param i32 i32 i32))

          (func (export "on_join") (param i64 i32 i32 i32 i32))

          (func (export "on_leave") (param i64 i32))

          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))

          (func (export "on_input") (param i64 i32 i32))

          (func (export "on_tick") (param f64) (result i64) (i64.const 0))

          (func (export "on_save") (result i64) (i64.const 8))

          (func (export "on_load") (param i32 i32))

          (func (export "on_is_full") (param i32) (result i32)

            (i32.ge_s (local.get 0) (i32.const 2)))

        )

    "#;

    pub(crate) fn wat_room() -> GameRoom {
        let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
        let module = runtime
            .compile(ABI_GUEST_WAT.as_bytes())
            .expect("compile guest wat");
        let linker = build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
        let pre = runtime.prepare(&linker, &module).expect("prepare guest");
        GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate guest room")
    }
