use crate::rooms::RoomHostData;

mod bulk;
mod core;
mod dynamics;
mod velocities;

pub(crate) fn register(
    linker: &mut wasmtime::Linker<RoomHostData>,
    permissions: engine_wasm::AbiPermissions,
) {
    core::register(linker, permissions);
    dynamics::register(linker, permissions);
    velocities::register(linker, permissions);
    bulk::register(linker, permissions);
}
