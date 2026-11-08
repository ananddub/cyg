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
                "sys_spatial_remove",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      x: f64,
                      y: f64|
                      -> i32 {
                    caller.data_mut().spatial.remove(entity_id as u32, [x, y]);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_spatial_remove");

        linker
            .func_wrap(
                ns,
                "sys_timer_set",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      timer_id: i64,
                      delay_ticks: i64,
                      interval_ticks: i64|
                      -> i32 {
                    caller
                        .data_mut()
                        .timer_set(timer_id, delay_ticks, interval_ticks);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_timer_set");

        linker
            .func_wrap(
                ns,
                "sys_timer_cancel",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, timer_id: i64| -> i32 {
                    caller.data_mut().timer_cancel(timer_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_timer_cancel");

        linker
            .func_wrap(
                ns,
                "sys_timer_poll",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>| -> i64 {
                    caller.data_mut().timer_poll()
                },
            )
            .expect("link sys_timer_poll");

        linker
            .func_wrap(
                ns,
                "sys_navmesh_find_path",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      sx: f64,
                      sy: f64,
                      dx: f64,
                      dy: f64,
                      out_ptr: i32,
                      max_points: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let Some(path) = caller
                        .data()
                        .navmesh_find_path((sx as f32, sy as f32), (dx as f32, dy as f32))
                    else {
                        return 0;
                    };
                    let count = if max_points > 0 {
                        (max_points as usize).min(path.len())
                    } else {
                        path.len()
                    };
                    if count > 0 && out_ptr > 0 {
                        let bytes_needed = count * 8;
                        let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory")
                        else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        };
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + bytes_needed > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        for (i, (px, py)) in path.iter().take(count).enumerate() {
                            let pos = offset + i * 8;
                            data[pos..pos + 4].copy_from_slice(&px.to_le_bytes());
                            data[pos + 4..pos + 8].copy_from_slice(&py.to_le_bytes());
                        }
                    }
                    count as i32
                },
            )
            .expect("link sys_navmesh_find_path");
    }
}
