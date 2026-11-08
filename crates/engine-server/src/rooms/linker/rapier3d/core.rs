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
                "sys_rapier3d_init",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      gx: f64,
                      gy: f64,
                      gz: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .init_rapier3d(gx as f32, gy as f32, gz as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_init");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_create_body",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_type: i32,
                      x: f64,
                      y: f64,
                      z: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller
                        .data_mut()
                        .rapier3d_create_body(body_type, x as f32, y as f32, z as f32)
                },
            )
            .expect("link sys_rapier3d_create_body");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_add_ball_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      radius: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller
                        .data_mut()
                        .rapier3d_add_ball_collider(body_id, radius as f32)
                },
            )
            .expect("link sys_rapier3d_add_ball_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_add_box_collider",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      hx: f64,
                      hy: f64,
                      hz: f64|
                      -> i32 {
                    if !permissions.physics {
                        return 0;
                    }
                    caller
                        .data_mut()
                        .rapier3d_add_box_collider(body_id, hx as f32, hy as f32, hz as f32)
                },
            )
            .expect("link sys_rapier3d_add_box_collider");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_set_position",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      x: f64,
                      y: f64,
                      z: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier3d_set_position(body_id, x as f32, y as f32, z as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_set_position");

        linker
            .func_wrap(
                ns,
                "sys_rapier3d_set_velocity",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      body_id: i32,
                      vx: f64,
                      vy: f64,
                      vz: f64|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    caller
                        .data_mut()
                        .rapier3d_set_velocity(body_id, vx as f32, vy as f32, vz as f32);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link sys_rapier3d_set_velocity");
    }
}
