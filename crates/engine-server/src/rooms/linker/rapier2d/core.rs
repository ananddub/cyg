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
                "sys_rapier2d_init",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, gx: f64, gy: f64| -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller.data_mut().init_rapier2d(gx as f32, gy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_init");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_create_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_type: i32,
                      x: f64,
                      y: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller
                        .data_mut()
                        .rapier2d_create_body(body_type, x as f32, y as f32)
                },
            )
            .expect("link sys_rapier2d_create_body");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_add_circle_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      radius: f64,
                      is_sensor: i32,
                      restitution: f64,
                      friction: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller.data_mut().rapier2d_add_circle_collider(
                        body_id,
                        radius as f32,
                        is_sensor != 0,
                        restitution as f32,
                        friction as f32,
                    )
                },
            )
            .expect("link sys_rapier2d_add_circle_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_add_box_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      half_w: f64,
                      half_h: f64,
                      is_sensor: i32,
                      restitution: f64,
                      friction: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller.data_mut().rapier2d_add_box_collider(
                        body_id,
                        half_w as f32,
                        half_h as f32,
                        is_sensor != 0,
                        restitution as f32,
                        friction as f32,
                    )
                },
            )
            .expect("link sys_rapier2d_add_box_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_set_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      vx: f64,
                      vy: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier2d_set_velocity(body_id, vx as f32, vy as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier2d_set_velocity");

        linker
            .func_wrap(
                ns,
                "sys_rapier2d_get_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let (vx, vy) = caller.data().rapier2d_get_velocity(body_id);
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 16 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&(vx as f64).to_le_bytes());
                            data[offset + 8..offset + 16]
                                .copy_from_slice(&(vy as f64).to_le_bytes());
                            return engine_wasm::ABI_OK;
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link sys_rapier2d_get_velocity");

    }
}
