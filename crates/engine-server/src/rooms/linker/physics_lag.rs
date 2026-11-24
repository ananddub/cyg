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
                "sys_physics_lag_record",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      _tick: i64,
                      entity_id: i32,
                      kind: i32,
                      min_x: f64,
                      min_y: f64,
                      min_z: f64,
                      max_x: f64,
                      max_y: f64,
                      max_z: f64| {
                    if !permissions.lag_compensation {
                        return;
                    }
                    // The guest's tick argument is not trusted — stamp the
                    // host clock so a guest cannot poison the lag history.
                    let tick = caller.data().clock_ticks.max(0) as u64;
                    let bounds = engine_physics::Aabb {
                        min: engine_physics::Vec3::new(min_x, min_y, min_z),
                        max: engine_physics::Vec3::new(max_x, max_y, max_z),
                    };
                    caller.data_mut().record_entity_hitbox(
                        tick,
                        0,
                        entity_id as u32,
                        kind as u32,
                        bounds,
                    );
                },
            )
            .expect("link sys_physics_lag_record");

        linker
            .func_wrap(
                ns,
                "sys_physics_lag_raycast",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      shoot_tick: i64,
                      ox: f64,
                      oy: f64,
                      oz: f64,
                      dx: f64,
                      dy: f64,
                      dz: f64,
                      max_dist: f64,
                      out_ptr: i32|
                      -> i32 {
                    if !permissions.lag_compensation {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    let origin = engine_physics::Vec3::new(ox, oy, oz);
                    let dir = engine_physics::Vec3::new(dx, dy, dz);

                    if let Some(hit) = caller.data_mut().lag_compensated_raycast(
                        shoot_tick as u64,
                        origin,
                        dir,
                        max_dist,
                    ) {
                        if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                            let data = mem.data_mut(&mut caller);
                            let offset = out_ptr as usize;
                            if offset + 44 <= data.len() {
                                data[offset..offset + 8]
                                    .copy_from_slice(&hit.point.x.to_le_bytes());
                                data[offset + 8..offset + 16]
                                    .copy_from_slice(&hit.point.y.to_le_bytes());
                                data[offset + 16..offset + 24]
                                    .copy_from_slice(&hit.point.z.to_le_bytes());
                                data[offset + 24..offset + 32]
                                    .copy_from_slice(&hit.distance.to_le_bytes());
                                data[offset + 32..offset + 36]
                                    .copy_from_slice(&hit.entity_id.to_le_bytes());
                                data[offset + 36..offset + 40]
                                    .copy_from_slice(&hit.kind.to_le_bytes());
                                data[offset + 40..offset + 44]
                                    .copy_from_slice(&hit.collider_id.to_le_bytes());
                            } else {
                                return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                            }
                        }
                        hit.hit_type as i32
                    } else {
                        0
                    }
                },
            )
            .expect("link sys_physics_lag_raycast");
    }
}
