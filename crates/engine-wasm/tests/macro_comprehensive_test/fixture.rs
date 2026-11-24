use engine_wasm::{WasmRuntime, wasm_module};

wasm_module! {
    pub struct BattleRoom {
        fn reset();
        fn set_speed(speed: f32);
        fn spawn_entity(entity_id: u32, x: f32, y: f32, team: i32) -> u32;
        fn calculate_exp(base: i32, multiplier: f32) -> i32;
        fn cause_trap();
    }
}

#[derive(Default)]
pub struct ServerSessionState {
    pub events_count: u32,
}

const WAT: &str = r#"
    (module
        (import "env" "notify_host" (func $notify_host))
        (memory (export "memory") 1)

        (global $speed (mut f32) (f32.const 100.0))
        (global $entity_count (mut i32) (i32.const 0))

        ;; 0 args
        (func (export "reset")
            (global.set $speed (f32.const 0.0))
            (global.set $entity_count (i32.const 0))
            (call $notify_host)
        )

        ;; 1 arg
        (func (export "set_speed") (param $s f32)
            (global.set $speed (local.get $s))
        )

        ;; 4 args with return value
        (func (export "spawn_entity") (param $id i32) (param $x f32) (param $y f32) (param $team i32) (result i32)
            (global.set $entity_count (i32.add (global.get $entity_count) (i32.const 1)))
            ;; Return compound hash id
            (i32.add (local.get $id) (local.get $team))
        )

        ;; 2 args math calculation
        (func (export "calculate_exp") (param $b i32) (param $m f32) (result i32)
            (i32.trunc_f32_s
                (f32.mul (f32.convert_i32_s (local.get $b)) (local.get $m))
            )
        )

        ;; Intentional trap (unreachable instruction)
        (func (export "cause_trap")
            (unreachable)
        )
    )
    "#;

pub fn open_room() -> BattleRoom<ServerSessionState> {
    let runtime = WasmRuntime::new().expect("Runtime initialization");

    let wasm_bytes = wat::parse_str(WAT).expect("WAT parse ok");
    let module = runtime.compile(&wasm_bytes).expect("Compile ok");

    let mut linker = runtime.create_linker::<ServerSessionState>();
    linker
        .func_wrap(
            "env",
            "notify_host",
            |mut caller: wasmtime::Caller<'_, ServerSessionState>| {
                caller.data_mut().events_count += 1;
            },
        )
        .expect("Linker wrap ok");

    let initial_state = ServerSessionState { events_count: 0 };
    BattleRoom::from_module_with_linker(&runtime, &linker, &module, initial_state)
        .expect("Instantiation ok")
}
