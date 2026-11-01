use super::*;

    const LIFECYCLE_GUEST_WAT: &str = r#"
        (module

          (memory (export "memory") 1)

          (global $ticks (mut i32) (i32.const 0))

          (global $leaves (mut i32) (i32.const 0))

          (global $reconnects (mut i32) (i32.const 0))

          (global $disposals (mut i32) (i32.const 0))

          (func (export "on_create") (param i32))

          (func (export "on_join") (param i64 f64 f64))

          (func (export "on_leave") (param i64)

            (global.set $leaves (i32.add (global.get $leaves) (i32.const 1))))

          (func (export "on_alloc") (param i32) (result i32) (i32.const 1024))

          (func (export "on_input") (param i64 i32 i32))

          (func (export "on_tick") (param f64) (result i64)

            (global.set $ticks (i32.add (global.get $ticks) (i32.const 1)))

            (i64.const 0))

          (func (export "on_save") (result i64) (i64.const 0))

          (func (export "on_load") (param i32 i32))

          (func (export "on_auth") (param i64 i32 i32) (result i32)

            (i32.and

              (i32.gt_s (local.get 2) (i32.const 0))

              (i32.eq (i32.load8_u (i32.const 1024)) (i32.const 123))))

          (func (export "on_allow_reconnection") (param i64) (result i32) (i32.const 5000))

          (func (export "on_reconnect") (param i64)

            (global.set $reconnects (i32.add (global.get $reconnects) (i32.const 1))))

          (func (export "on_dispose")

            (global.set $disposals (i32.add (global.get $disposals) (i32.const 1))))

          (func (export "on_patch_rate") (result i32) (i32.const 100))

          (func (export "on_simulation_rate") (result i32) (i32.const 50))

          (func (export "tick_count") (result i32) (global.get $ticks))

          (func (export "leave_count") (result i32) (global.get $leaves))

          (func (export "reconnect_count") (result i32) (global.get $reconnects))

          (func (export "dispose_count") (result i32) (global.get $disposals))

        )

    "#;

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

          (func (export "on_create") (param i32))

          (func (export "on_join") (param i64 f64 f64))

          (func (export "on_leave") (param i64))

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
