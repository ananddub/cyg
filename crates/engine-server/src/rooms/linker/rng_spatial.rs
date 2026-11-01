use crate::rooms::RoomHostData;

pub(crate) fn register(
    linker: &mut wasmtime::Linker<RoomHostData>,
    _permissions: engine_wasm::AbiPermissions,
) {
    for ns in &[
        engine_wasm::HOST_NAMESPACE_V1,
        engine_wasm::HOST_NAMESPACE_LEGACY,
        "ox",
    ] {

        linker
            .func_wrap(
                ns,
                "sys_rng_next",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>| -> f64 {
                    caller.data_mut().rng.next_f32() as f64
                },
            )
            .expect("link sys_rng_next");

        linker
            .func_wrap(
                ns,
                "sys_clock_ticks",
                move |caller: wasmtime::Caller<'_, RoomHostData>| -> i64 {
                    caller.data().clock_ticks
                },
            )
            .expect("link sys_clock_ticks");

        linker
            .func_wrap(
                ns,
                "sys_spatial_update",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      x: f64,
                      y: f64|
                      -> i32 {
                    caller.data_mut().spatial.upsert(entity_id as u32, [x, y]);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_spatial_update");

        linker
            .func_wrap(
                ns,
                "sys_spatial_query",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      x: f64,
                      y: f64,
                      radius: f64,
                      out_ptr: i32,
                      max_results: i32|
                      -> i32 {
                    let hits = caller.data().spatial.query_radius([x, y], radius);
                    let limit = if max_results > 0 {
                        (max_results as usize).min(hits.len())
                    } else {
                        hits.len()
                    };
                    if limit > 0 && out_ptr > 0 {
                        let offset = out_ptr as usize;
                        let bytes_needed = limit * 4;
                        let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory")
                        else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        };
                        let data = mem.data_mut(&mut caller);
                        if offset + bytes_needed > data.len() {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                        for (i, hit) in hits.iter().take(limit).enumerate() {
                            let pos = offset + i * 4;
                            data[pos..pos + 4]
                                .copy_from_slice(&(hit.entity_id as u32).to_le_bytes());
                        }
                    }
                    limit as i32
                },
            )
            .expect("link sys_spatial_query");

    }
}
