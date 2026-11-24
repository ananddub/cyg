use crate::netcode::{MAX_VIEW_INTENTS, ViewIntent};
use crate::rooms::{
    MAX_OUTBOX_MESSAGES, MAX_STAT_INTENTS, OutboundMessage, RoomHostData, StatIntent,
};

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
                "abort",
                |_caller: wasmtime::Caller<'_, RoomHostData>,
                 _m: u32,
                 _f: u32,
                 _l: u32,
                 _c: u32| {},
            )
            .expect("link abort");

        let _ = linker.func_wrap(
            ns,
            "_ZdlPvm",
            |_c: wasmtime::Caller<'_, RoomHostData>, _ptr: i32, _sz: i32| {},
        );
        let _ = linker.func_wrap(
            ns,
            "_ZdlPv",
            |_c: wasmtime::Caller<'_, RoomHostData>, _ptr: i32| {},
        );

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

        // Per-player counters. The guest names a seat and a counter; the host
        // checks the permission, the name shape and the delta, then queues an
        // intent. No user id, table or SQL ever crosses this boundary.
        linker
            .func_wrap(
                ns,
                "host_stat_add",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      seat: i64,
                      stat_ptr: i32,
                      stat_len: i32,
                      delta: i64|
                      -> i32 {
                    if !permissions.stats {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    const MAX_STAT_NAME: i32 = 64;
                    const MAX_STAT_DELTA: i64 = 1_000_000;
                    if seat < 0 || stat_len <= 0 || stat_len > MAX_STAT_NAME {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    if delta <= 0 || delta > MAX_STAT_DELTA {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let start = stat_ptr as usize;
                    let end = start.saturating_add(stat_len as usize);
                    if start >= memory.len() || end > memory.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    let name = match std::str::from_utf8(&memory[start..end]) {
                        Ok(name) if valid_stat_name(name) => name,
                        Ok(_) => return engine_wasm::ABI_ERR_INVALID_ARGUMENT,
                        Err(_) => return engine_wasm::ABI_ERR_INVALID_UTF8,
                    };
                    let stat: std::sync::Arc<str> = std::sync::Arc::from(name);
                    let data = caller.data_mut();
                    if data.stat_intents.len() >= MAX_STAT_INTENTS {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.stat_intents.push(StatIntent { seat, stat, delta });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_stat_add");

        // Per-session state views. The guest can only subtract: hide a named
        // field, drop every row but the viewer's own, or reset to the default.
        // `.private()` still wins over every view. The host resolves names
        // against the descriptor and forces a fresh full snapshot on change.
        linker
            .func_wrap(
                ns,
                "host_view_hide",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      seat: i64,
                      collection_ptr: i32,
                      collection_len: i32,
                      field_ptr: i32,
                      field_len: i32|
                      -> i32 {
                    if !permissions.views {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if seat <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let collection = match read_view_name(memory, collection_ptr, collection_len) {
                        Ok(name) => name,
                        Err(code) => return code,
                    };
                    let field = match read_view_name(memory, field_ptr, field_len) {
                        Ok(name) => name,
                        Err(code) => return code,
                    };
                    let data = caller.data_mut();
                    if data.view_intents.len() >= MAX_VIEW_INTENTS {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.view_intents.push(ViewIntent::HideField {
                        seat,
                        collection,
                        field,
                    });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_view_hide");

        linker
            .func_wrap(
                ns,
                "host_view_only_self",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      seat: i64,
                      enabled: i32|
                      -> i32 {
                    if !permissions.views {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if seat <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let data = caller.data_mut();
                    if data.view_intents.len() >= MAX_VIEW_INTENTS {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.view_intents.push(ViewIntent::OnlySelf {
                        seat,
                        enabled: enabled != 0,
                    });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_view_only_self");

        linker
            .func_wrap(
                ns,
                "host_view_reset",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, seat: i64| -> i32 {
                    if !permissions.views {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if seat <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let data = caller.data_mut();
                    if data.view_intents.len() >= MAX_VIEW_INTENTS {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    data.view_intents.push(ViewIntent::Reset { seat });
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_view_reset");

        linker
            .func_wrap(
                ns,
                "host_rewind_record",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      tick: i64,
                      values_ptr: i32,
                      count: i32|
                      -> i32 {
                    if !permissions.rewind {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if entity_id <= 0
                        || tick < 0
                        || count <= 0
                        || count as usize > crate::rooms::MAX_REWIND_FIELDS
                    {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let start = values_ptr as usize;
                    let end = start.saturating_add(count as usize * 8);
                    if start >= memory.len() || end > memory.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    let mut values = Vec::with_capacity(count as usize);
                    for chunk in memory[start..end].chunks_exact(8) {
                        values.push(f64::from_le_bytes(chunk.try_into().unwrap()));
                    }
                    let data = caller.data_mut();
                    match data
                        .rewind
                        .entry(entity_id)
                        .or_default()
                        .record(tick, values)
                    {
                        Ok(()) => engine_wasm::ABI_OK,
                        Err(_) => engine_wasm::ABI_ERR_INVALID_ARGUMENT,
                    }
                },
            )
            .expect("link host_rewind_record");

        linker
            .func_wrap(
                ns,
                "host_rewind_record_time",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      tick: i64,
                      time_ms: f64,
                      values_ptr: i32,
                      count: i32|
                      -> i32 {
                    if !permissions.rewind {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if entity_id <= 0
                        || tick < 0
                        || count <= 0
                        || count as usize > crate::rooms::MAX_REWIND_FIELDS
                        || !time_ms.is_finite()
                    {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data(&caller);
                    let start = values_ptr as usize;
                    let end = start.saturating_add(count as usize * 8);
                    if start >= memory.len() || end > memory.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    let mut values = Vec::with_capacity(count as usize);
                    for chunk in memory[start..end].chunks_exact(8) {
                        values.push(f64::from_le_bytes(chunk.try_into().unwrap()));
                    }
                    let data = caller.data_mut();
                    match data
                        .rewind
                        .entry(entity_id)
                        .or_default()
                        .record_time(tick, time_ms, values)
                    {
                        Ok(()) => engine_wasm::ABI_OK,
                        Err(_) => engine_wasm::ABI_ERR_INVALID_ARGUMENT,
                    }
                },
            )
            .expect("link host_rewind_record_time");

        linker
            .func_wrap(
                ns,
                "host_rewind_query",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      tick: i64,
                      out_ptr: i32,
                      out_len: i32|
                      -> i32 {
                    if !permissions.rewind {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if entity_id <= 0 || tick < 0 || out_len <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let data = caller.data();
                    let Some(track) = data.rewind.get(&entity_id) else {
                        return engine_wasm::ABI_ERR_GENERIC;
                    };
                    let Some(values) = track.at(tick) else {
                        return engine_wasm::ABI_ERR_GENERIC;
                    };
                    if values.len() > out_len as usize {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    let values = values.to_vec();
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data_mut(&mut caller);
                    let start = out_ptr as usize;
                    let end = start.saturating_add(values.len() * 8);
                    if start >= memory.len() || end > memory.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    for (index, value) in values.iter().enumerate() {
                        let at = start + index * 8;
                        memory[at..at + 8].copy_from_slice(&value.to_le_bytes());
                    }
                    values.len() as i32
                },
            )
            .expect("link host_rewind_query");

        linker
            .func_wrap(
                ns,
                "host_rewind_query_time",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>,
                      entity_id: i64,
                      time_ms: f64,
                      out_ptr: i32,
                      out_len: i32|
                      -> i32 {
                    if !permissions.rewind {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if entity_id <= 0 || !time_ms.is_finite() || out_len <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    let data = caller.data();
                    let Some(track) = data.rewind.get(&entity_id) else {
                        return engine_wasm::ABI_ERR_GENERIC;
                    };
                    let Some(values) = track.at_time(time_ms) else {
                        return engine_wasm::ABI_ERR_GENERIC;
                    };
                    if values.len() > out_len as usize {
                        return engine_wasm::ABI_ERR_BUFFER_OVERFLOW;
                    }
                    let Some(wasmtime::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    };
                    let memory = mem.data_mut(&mut caller);
                    let start = out_ptr as usize;
                    let end = start.saturating_add(values.len() * 8);
                    if start >= memory.len() || end > memory.len() {
                        return engine_wasm::ABI_ERR_OUT_OF_BOUNDS;
                    }
                    for (index, value) in values.iter().enumerate() {
                        let at = start + index * 8;
                        memory[at..at + 8].copy_from_slice(&value.to_le_bytes());
                    }
                    values.len() as i32
                },
            )
            .expect("link host_rewind_query_time");

        linker
            .func_wrap(
                ns,
                "host_rewind_clear",
                move |mut caller: wasmtime::Caller<'_, RoomHostData>, entity_id: i64| -> i32 {
                    if !permissions.rewind {
                        return engine_wasm::ABI_ERR_PERMISSION_DENIED;
                    }
                    if entity_id <= 0 {
                        return engine_wasm::ABI_ERR_INVALID_ARGUMENT;
                    }
                    caller.data_mut().rewind.remove(&entity_id);
                    engine_wasm::ABI_OK
                },
            )
            .expect("link host_rewind_clear");

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

/// A counter name is the same lowercase identifier the control plane and the
/// quest catalog use. Refusing anything else here keeps a typo or a hostile
/// name from ever reaching the database.
fn valid_stat_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A collection or field name from guest memory. The same identifier subset
/// the schema parser and stat names use — no whitespace, dots or SQL.
fn read_view_name(memory: &[u8], ptr: i32, len: i32) -> Result<std::sync::Arc<str>, i32> {
    if len <= 0 || len > 64 {
        return Err(engine_wasm::ABI_ERR_INVALID_ARGUMENT);
    }
    let start = ptr as usize;
    let end = start.saturating_add(len as usize);
    if start >= memory.len() || end > memory.len() {
        return Err(engine_wasm::ABI_ERR_OUT_OF_BOUNDS);
    }
    let name =
        std::str::from_utf8(&memory[start..end]).map_err(|_| engine_wasm::ABI_ERR_INVALID_UTF8)?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(engine_wasm::ABI_ERR_INVALID_ARGUMENT);
    }
    Ok(std::sync::Arc::from(name))
}
