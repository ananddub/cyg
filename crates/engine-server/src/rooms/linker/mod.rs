use crate::rooms::RoomHostData;

mod physics;
mod physics_lag;
mod rapier2d;
mod rapier3d;
mod rng_spatial;
mod sys;
mod timers;

pub fn build_host_linker(
    runtime: &engine_wasm::WasmRuntime,
    permissions: engine_wasm::AbiPermissions,
) -> wasmtime::Linker<RoomHostData> {
    let mut linker = runtime.create_linker::<RoomHostData>();
    sys::register(&mut linker, permissions);
    physics::register(&mut linker, permissions);
    physics_lag::register(&mut linker, permissions);
    rng_spatial::register(&mut linker, permissions);
    rapier2d::register(&mut linker, permissions);
    timers::register(&mut linker, permissions);
    rapier3d::register(&mut linker, permissions);
    linker
}
