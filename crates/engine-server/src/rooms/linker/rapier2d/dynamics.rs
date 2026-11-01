use crate::rooms::RoomHostData;

pub(crate) fn register(
    linker: &mut wasmtime::Linker<RoomHostData>,
    permissions: engine_wasm::AbiPermissions,
) {
    for ns in &[
        engine_wasm::HOST_NAMESPACE_V1,
        engine_wasm::HOST_NAMESPACE_LEGACY,
        "ox",
    ] {

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_get_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let Some((x, y)) = caller.data().rapier2d_get_position(body_id) else {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    };
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 16 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&(x as f64).to_le_bytes());
                            data[offset + 8..offset + 16]
                                .copy_from_slice(&(y as f64).to_le_bytes());
                            return engine_wasm::ABI_OK;
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link sys_rapier2d_get_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_set_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      x: f64,
                      y: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier2d_set_position(body_id, x as f32, y as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_set_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_apply_impulse",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      ix: f64,
                      iy: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier2d_apply_impulse(body_id, ix as f32, iy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_apply_impulse");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_step",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, dt: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_step(dt as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_step");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_check_contact",
                move |caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id_a: i32,
                      body_id_b: i32|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    if caller.data().rapier2d_check_contact(body_id_a, body_id_b) {
                        1
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_rapier2d_check_contact");

    }
}
