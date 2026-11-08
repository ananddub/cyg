use crate::rooms::{MAX_OUTBOX_MESSAGES, OutboundMessage, RoomHostData};

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
                "abort",
                |_caller: wasmtime::Caller<'_, RoomHostData>,
                 _m: u32,
                 _f: u32,
                 _l: u32,
                 _c: u32| {},
            )
            .expect("link abort");

        linker
            .func_wrap(
                ns,
                "host_kick",
                |mut caller: wasmtime::Caller<'_, RoomHostData>, player_id: i64| -> i32 {
                    let data = caller.data_mut();
                    if data.kick_requests.len() >= MAX_OUTBOX_MESSAGES {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.kick_requests.push(player_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_kick");

        linker
            .func_wrap(
                ns,
                "host_log",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, ptr: i32, len: i32| -> i32 {
                    if !permissions.logging {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if len <= 0 {
                        return engine_wasm::ABI_OK;
                    }
                    let len = len as usize;
                    if len > engine_wasm::MAX_LOG_LENGTH {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    if let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") {
                        let data = mem.data(&caller);
                        let offset = ptr as usize;
                        if offset + len <= data.len() {
                            if let Ok(msg) = std::str::from_utf8(&data[offset..offset + len]) {
                                tracing::info!("[wasm-log] {msg}");
                                return engine_wasm::ABI_OK;
                            } else {
                                return engine_wasm::ABI_ERR_INVALID_UTF8;
                            }
                        }
                    }
                    engine_wasm::ABI_ERR_OUT_OF_BOUNDS
                },
            )
            .expect("link host_log");

        linker
            .func_wrap(
                ns,
                "host_send",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      target: i64,
                      type_ptr: i32,
                      type_len: i32,
                      data_ptr: i32,
                      data_len: i32|
                      -> i32 {
                    if !permissions.messaging {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if type_len <= 0
                        || type_len > 255
                        || data_len < 0
                        || data_len as usize > engine_wasm::MAX_INBOUND_PAYLOAD_SIZE
                    {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }

                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let t_off = type_ptr as usize;
                    let t_end = t_off.saturating_add(type_len as usize);
                    let d_off = data_ptr as usize;
                    let d_end = d_off.saturating_add(data_len as usize);
                    if t_off >= memory.len()
                        || t_end > memory.len()
                        || (data_len > 0 && (d_off >= memory.len() || d_end > memory.len()))
                    {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }

                    let event = match std::str::from_utf8(&memory[t_off..t_end]) {
                        Ok(name) => std::sync::Arc::from(name),
                        Err(_) => return engine_wasm::ABI_ERR_INVALID_UTF8,
                    };
                    let payload = if data_len > 0 {
                        bytes::Bytes::copy_from_slice(&memory[d_off..d_end])
                    } else {
                        bytes::Bytes::new()
                    };

                    let data = caller.data_mut();
                    if data.outbox.len() >= MAX_OUTBOX_MESSAGES {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.outbox.push(OutboundMessage {
                        target,
                        event,
                        payload,
                    });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_send");

        linker
            .func_wrap(
                ns,
                "get_abi_version",
                move |_caller: wasmtime::Caller<'_, RoomHostData>| -> i32 {
                    engine_wasm::CURRENT_ABI_VERSION as i32
                },
            )
            .expect("link get_abi_version");
    }
}
