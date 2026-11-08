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
                "sys_rapier3d_get_positions",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      in_ptr: i32,
                      out_ptr: i32,
                      count: i32,
                      other_body: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if count <= 0 || count > 16_384 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let bodies: Vec<i32> = {
                        let data = mem.data_mut(&mut caller);
                        let in_start = in_ptr as usize;
                        if in_start + count as usize * 4 > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        (0..count as usize)
                            .map(|i| {
                                let off = in_start + i * 4;
                                i32::from_le_bytes(data[off..off + 4].try_into().unwrap())
                            })
                            .collect()
                    };
                    let resolved: Vec<([f64; 3], u32)> = {
                        let host = caller.data();
                        bodies
                            .iter()
                            .map(|body_id| {
                                let pos =
                                    host.rapier3d_get_position(*body_id).unwrap_or([0.0f32; 3]);
                                let contact = if other_body >= 0
                                    && host.rapier3d_check_contact(*body_id, other_body)
                                {
                                    1u32
                                } else {
                                    0
                                };
                                ([pos[0] as f64, pos[1] as f64, pos[2] as f64], contact)
                            })
                            .collect()
                    };
                    let data = mem.data_mut(&mut caller);
                    let out_start = out_ptr as usize;
                    if out_start + count as usize * 32 > data.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    for (i, (pos, contact)) in resolved.iter().enumerate() {
                        let off = out_start + i * 32;
                        data[off..off + 8].copy_from_slice(&pos[0].to_le_bytes());
                        data[off + 8..off + 16].copy_from_slice(&pos[1].to_le_bytes());
                        data[off + 16..off + 24].copy_from_slice(&pos[2].to_le_bytes());
                        data[off + 24..off + 28].copy_from_slice(&contact.to_le_bytes());
                    }
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_get_positions");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_remove_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier3d_remove_body(body_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_remove_body");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_step",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, dt: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier3d_step(dt as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_step");
    }
}
