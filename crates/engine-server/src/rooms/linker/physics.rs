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
                "sys_physics_surface_y",
                move |caller: wasmtime::Caller<'_, RoomHostData>,
                      px: f64,
                      py: f64,
                      pz: f64,
                      radius: f64|
                      -> f64 {
                    if !permissions.physics {
                        return 0.0;
                    }
                    caller.data().get_surface_height(px, py, pz, radius)
                },
            )
            .expect("link sys_physics_surface_y");

        linker
            .func_wrap(
                ns,
                "sys_physics_move",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      px: f64,
                      py: f64,
                      pz: f64,
                      vx: f64,
                      vy: f64,
                      vz: f64,
                      radius: f64,
                      dt: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let res = caller
                        .data()
                        .move_and_slide(px, py, pz, vx, vy, vz, radius, dt);

                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data_mut(&mut caller);
                        let offset = out_ptr as usize;
                        if offset + 24 <= data.len() {
                            data[offset..offset + 8].copy_from_slice(&res.new_x.to_le_bytes());
                            data[offset + 8..offset + 16].copy_from_slice(&res.new_y.to_le_bytes());
                            data[offset + 16..offset + 24]
                                .copy_from_slice(&res.new_z.to_le_bytes());
                        } else {
                            return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                        }
                    }

                    if res.is_grounded { 1 } else { 0 }
                },
            )
            .expect("link sys_physics_move");

        linker
            .func_wrap(
                ns,
                "sys_physics_raycast",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      ox: f64,
                      oy: f64,
                      oz: f64,
                      dx: f64,
                      dy: f64,
                      dz: f64,
                      max_dist: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.physics {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

                    let res = if let Some(hit) = caller.data().raycast(origin, dir, max_dist) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 36 <= data.len() {
                                data[offset..offset + 8]
                                    .copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16]
                                    .copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24]
                                    .copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32]
                                    .copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36]
                                    .copy_from_slice(&hit.collider_id.to_le_bytes());
                            } else {
                                return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                            }
                        }
                        1
                    } else {
                        0
                    };
                    res
                },
            )
            .expect("link sys_physics_raycast");
    }
}
