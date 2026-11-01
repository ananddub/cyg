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
                "sys_rapier3d_get_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let Some([x, y, z]) = caller.data().rapier3d_get_position(body_id) else {
                        return 0;
                    };
                    if out_ptr <= 0 {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let data = mem.data_mut(&mut caller);
                    let offset = out_ptr as usize;
                    if offset + 12 > data.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    data[offset..offset + 4].copy_from_slice(&x.to_le_bytes());
                    data[offset + 4..offset + 8].copy_from_slice(&y.to_le_bytes());
                    data[offset + 8..offset + 12].copy_from_slice(&z.to_le_bytes());
                    1
                },
            )
            .expect("link sys_rapier3d_get_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_get_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let [x, y, z] = caller.data().rapier3d_get_velocity(body_id);
                    if out_ptr <= 0 {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let data = mem.data_mut(&mut caller);
                    let offset = out_ptr as usize;
                    if offset + 12 > data.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    data[offset..offset + 4].copy_from_slice(&x.to_le_bytes());
                    data[offset + 4..offset + 8].copy_from_slice(&y.to_le_bytes());
                    data[offset + 8..offset + 12].copy_from_slice(&z.to_le_bytes());
                    1
                },
            )
            .expect("link sys_rapier3d_get_velocity");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_apply_impulse",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      ix: f64,
                      iy: f64,
                      iz: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier3d_apply_impulse(body_id, ix as f32, iy as f32, iz as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_apply_impulse");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_check_contact",
                move |caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id_a: i32,
                      body_id_b: i32|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    if caller.data().rapier3d_check_contact(body_id_a, body_id_b) {
                        1
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_rapier3d_check_contact");

    }
}
