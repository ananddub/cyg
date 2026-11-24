use crate::rooms::RoomHostData;

pub(crate) fn register(
    linker: &mut wasmtime::Linker<RoomHostData>,
    permissions: engine_wasm::AbiPermissions,
) {
    for ns in &[
        engine_wasm::HOST_NAMESPACE_V1,
        engine_wasm::HOST_NAMESPACE_LEGACY,
        "cyg",
        "ox",
    ] {
        linker
            .func_wrap(
                ns,
                "sys_rapier2d_set_velocities",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      ptr: i32,
                      count: i32|
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
                    let batch: Vec<(i32, f64, f64)> = {
                        let data = mem.data_mut(&mut caller);
                        let start = ptr as usize;
                        let need = count as usize * 20;
                        if start + need > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        (0..count as usize)
                            .map(|i| {
                                let off = start + i * 20;
                                (
                                    i32::from_le_bytes(data[off..off + 4].try_into().unwrap()),
                                    f64::from_le_bytes(data[off + 4..off + 12].try_into().unwrap()),
                                    f64::from_le_bytes(
                                        data[off + 12..off + 20].try_into().unwrap(),
                                    ),
                                )
                            })
                            .collect()
                    };
                    let host = caller.data_mut();
                    for (body_id, vx, vy) in &batch {
                        host.rapier2d_set_velocity(*body_id, *vx as f32, *vy as f32);
                    }
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_set_velocities");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_get_positions",
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
                    let resolved: Vec<(f64, f64, u32)> = {
                        let host = caller.data();
                        bodies
                            .iter()
                            .map(|body_id| {
                                let (x, y) = match host.rapier2d_get_position(*body_id) {
                                    Some((x, y)) => (x as f64, y as f64),
                                    None => (0.0, 0.0),
                                };
                                let contact = if other_body >= 0
                                    && host.rapier2d_check_contact(*body_id, other_body)
                                {
                                    1u32
                                } else {
                                    0
                                };
                                (x, y, contact)
                            })
                            .collect()
                    };
                    let data = mem.data_mut(&mut caller);
                    let out_start = out_ptr as usize;
                    if out_start + count as usize * 24 > data.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    for (i, (x, y, contact)) in resolved.iter().enumerate() {
                        let off = out_start + i * 24;
                        data[off..off + 8].copy_from_slice(&x.to_le_bytes());
                        data[off + 8..off + 16].copy_from_slice(&y.to_le_bytes());
                        data[off + 16..off + 20].copy_from_slice(&contact.to_le_bytes());
                    }
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_get_positions");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_remove_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, body_id: i32| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().rapier2d_remove_body(body_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_remove_body");
    }
}
