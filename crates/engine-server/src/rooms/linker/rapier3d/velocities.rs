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
                "sys_rapier3d_set_velocities",
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
                    let batch: Vec<(i32, f64, f64, f64)> = {
                        let data = mem.data_mut(&mut caller);
                        let start = ptr as usize;
                        let need = count as usize * 28;
                        if start + need > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        (0..count as usize)
                            .map(|i| {
                                let off = start + i * 28;
                                (
                                    i32::from_le_bytes(data[off..off + 4].try_into().unwrap()),
                                    f64::from_le_bytes(data[off + 4..off + 12].try_into().unwrap()),
                                    f64::from_le_bytes(
                                        data[off + 12..off + 20].try_into().unwrap(),
                                    ),
                                    f64::from_le_bytes(
                                        data[off + 20..off + 28].try_into().unwrap(),
                                    ),
                                )
                            })
                            .collect()
                    };
                    let host = caller.data_mut();
                    for (body_id, vx, vy, vz) in &batch {
                        host.rapier3d_set_velocity(*body_id, *vx as f32, *vy as f32, *vz as f32);
                    }
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_set_velocities");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_get_velocities",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      in_ptr: i32,
                      out_ptr: i32,
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
                    let resolved: Vec<(f64, f64, f64)> = {
                        let host = caller.data();
                        bodies
                            .iter()
                            .map(|body_id| {
                                let v = host.rapier3d_get_velocity(*body_id);
                                (v[0] as f64, v[1] as f64, v[2] as f64)
                            })
                            .collect()
                    };
                    let data = mem.data_mut(&mut caller);
                    let out_start = out_ptr as usize;
                    if out_start + count as usize * 24 > data.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    for (i, (vx, vy, vz)) in resolved.iter().enumerate() {
                        let off = out_start + i * 24;
                        data[off..off + 8].copy_from_slice(&vx.to_le_bytes());
                        data[off + 8..off + 16].copy_from_slice(&vy.to_le_bytes());
                        data[off + 16..off + 24].copy_from_slice(&vz.to_le_bytes());
                    }
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_get_velocities");
    }
}
