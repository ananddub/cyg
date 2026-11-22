//! C ABI — the contract every native binding (Unity/C#, Kotlin, Swift, Godot)
//! binds against. Keep this surface small and stable: it is the product.
//!
//! Ownership rules:
//! - strings/bytes passed IN are caller-owned, read-only;
//! - bytes written OUT go into caller-provided buffers;
//! - handles are u32 slots; `cyg_free` releases one.
//!
//! Version the ABI: `cyg_abi_version()` lets bindings assert they were built
//! against the same core.

use std::ffi::{c_char, CStr};

use crate::ClientCore;
use crate::state::Value;

struct Slots {
    cores: Vec<Option<ClientCore>>,
}

impl Slots {
    fn get() -> &'static std::sync::Mutex<Slots> {
        static SLOTS: std::sync::OnceLock<std::sync::Mutex<Slots>> = std::sync::OnceLock::new();
        SLOTS.get_or_init(|| std::sync::Mutex::new(Slots { cores: Vec::new() }))
    }
}

const CYG_ABI_VERSION: u32 = 1;

/// ABI version of this core build; bindings must assert equality.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_abi_version() -> u32 {
    CYG_ABI_VERSION
}

/// Create a core from descriptor bytes. Returns a handle, or 0 on bad input.
/// # Safety
/// `descriptor` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_create(descriptor: *const u8, len: usize) -> u32 {
    if descriptor.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(descriptor, len) };
    match ClientCore::new(bytes) {
        Ok(core) => {
            let mut slots = Slots::get().lock().unwrap();
            slots.cores.push(Some(core));
            slots.cores.len() as u32
        }
        Err(_) => 0,
    }
}

/// Release a handle.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_free(handle: u32) {
    let mut slots = Slots::get().lock().unwrap();
    let idx = (handle as usize).checked_sub(1);
    if let Some(slot) = idx.and_then(|i| slots.cores.get_mut(i)) {
        *slot = None;
    }
}

fn with_core<R>(handle: u32, f: impl FnOnce(&mut ClientCore) -> R) -> Option<R> {
    let mut slots = Slots::get().lock().unwrap();
    slots.cores.get_mut(handle as usize - 1)?.as_mut().map(f)
}

/// Feed one raw room-data payload. Returns the number of state events
/// produced (>= 0), or -1 on a decode error.
/// # Safety
/// `payload` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_apply_payload(handle: u32, payload: *const u8, len: usize) -> i32 {
    if payload.is_null() {
        return -1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(payload, len) };
    with_core(handle, |core| match core.apply_payload(bytes) {
        Ok(events) => events.len() as i32,
        Err(_) => -1,
    })
    .unwrap_or(-1)
}

/// Field value of one entity by field NAME (schema order lookup).
/// Returns the f64 view of the value (numbers), NaN when absent/typemismatch.
/// # Safety
/// `field` must be a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_get_f64(handle: u32, entity_id: u32, field: *const c_char) -> f64 {
    let name = unsafe { CStr::from_ptr(field) }.to_str().unwrap_or("");
    with_core(handle, |core| -> Option<f64> {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        let index = core.descriptor.schemas.iter()
            .find(|s| engine_schema::wire_hash(s) == coll.schema_hash)
            .map(|s| s.fields.iter().position(|f| f.name == name))
            .flatten()?;
        match entity.values.get(index)? {
            Value::F64(v) => Some(*v),
            Value::F32(v) => Some(*v as f64),
            v => Some(v.as_f64()),
        }
    })
    .and_then(|r| r)
    .unwrap_or(f64::NAN)
}


/// f64 view of one entity field by schema-field INDEX (NaN = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_f64_by_index(handle: u32, entity_id: u32, index: usize) -> f64 {
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        match entity.values.get(index)? {
            Value::F64(v) => Some(*v),
            Value::F32(v) => Some(*v as f64),
            v => Some(v.as_f64()),
        }
    })
    .and_then(|r| r)
    .unwrap_or(f64::NAN)
}

/// String field of one entity: writes into `out` (cap bytes), returns length
/// written, or -1 when the field is not a string / handle invalid.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_string_by_index(
    handle: u32,
    entity_id: u32,
    index: usize,
    out: *mut u8,
    cap: usize,
) -> i32 {
    if out.is_null() {
        return -1;
    }
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        match entity.values.get(index)? {
            Value::Str(s) => {
                let bytes = s.as_bytes();
                let n = bytes.len().min(cap);
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, n) };
                Some(bytes.len() as i32)
            }
            _ => None,
        }
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

/// Bool field of one entity (0/1); -1 = absent/not bool.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_bool_by_index(handle: u32, entity_id: u32, index: usize) -> i32 {
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        match entity.values.get(index)? {
            Value::Bool(v) => Some(if *v { 1 } else { 0 }),
            _ => None,
        }
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

/// String length of one entity's string field (-1 = absent/not string).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_string_len(handle: u32, entity_id: u32, index: usize) -> i32 {
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        match entity.values.get(index)? {
            Value::Str(s) => Some(s.len() as i32),
            _ => None,
        }
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

/// Encode a schema-lane input frame into `out`; -2 = buffer too small.
/// Values are raw i64s reinterpreted per the Input schema's field kinds.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_encode_schema_input(
    handle: u32,
    seq: u16,
    values: *const i64,
    values_len: usize,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if values.is_null() || out.is_null() || out_len.is_null() {
        return -1;
    }
    let raw = unsafe { std::slice::from_raw_parts(values, values_len) };
    with_core(handle, |core| {
        let input_schema = core.descriptor.schemas.iter()
            .find(|s| s.name == engine_schema::INPUT_SCHEMA_NAME)
            .cloned();
        let Some(input_schema) = input_schema else { return -1 };
        if values_len != input_schema.fields.len() {
            return -1;
        }
        let mapped: Vec<crate::input::InputValue> = input_schema.fields.iter().zip(raw).map(|(field, v)| {
            use engine_schema::WireKind as K;
            match field.kind {
                K::Bool => crate::input::InputValue::Bool(*v != 0),
                K::U8 => crate::input::InputValue::U8(*v as u8),
                K::I8 => crate::input::InputValue::I8(*v as i8),
                K::U16 => crate::input::InputValue::U16(*v as u16),
                K::I16 => crate::input::InputValue::I16(*v as i16),
                K::U32 => crate::input::InputValue::U32(*v as u32),
                K::I32 => crate::input::InputValue::I32(*v as i32),
                K::I64 => crate::input::InputValue::I64(*v),
                K::F32 => crate::input::InputValue::F32(f32::from_bits(*v as u32)),
                K::F64 => crate::input::InputValue::F64(f64::from_bits(*v as u64)),
                _ => crate::input::InputValue::U8(0),
            }
        }).collect();
        match crate::input::pack_schema_input(seq, &mapped, &input_schema.fields) {
            Ok(wire) => {
                if wire.len() > out_cap {
                    return -2;
                }
                unsafe { std::ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len()) };
                unsafe { *out_len = wire.len(); }
                0
            }
            Err(_) => -1,
        }
    })
    .unwrap_or(-1)
}


/// Entity id at logical position `pos` (iteration order = ascending id).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_entity_id_at(handle: u32, pos: u32) -> u32 {
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        coll.entities.keys().nth(pos as usize).copied()
    })
    .and_then(|r| r)
    .unwrap_or(u32::MAX)
}

/// Schema-field index by NAME (for typed getters); -1 = not found.
/// # Safety
/// `field` must be a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_field_index(handle: u32, field: *const u8, field_len: usize) -> i32 {
    if field.is_null() {
        return -1;
    }
    let name = unsafe { std::slice::from_raw_parts(field, field_len) };
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        core.descriptor.schemas.iter()
            .find(|s| engine_schema::wire_hash(s) == coll.schema_hash)
            .and_then(|s| s.fields.iter().position(|f| f.name.as_bytes() == name))
            .map(|i| i as i32)
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}


/// Entity count in the primary collection.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_entity_count(handle: u32) -> u32 {
    with_core(handle, |core| {
        core.state().collections.first()
            .map(|c| c.entities.len() as u32)
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Encode a schema-lane input frame into `out`; `out_len` receives the length.
/// Values are passed as raw little-endian i64s and reinterpreted per the
/// Input schema's field kinds (f64 fields get their bit pattern via f64 cast
/// by the caller: pass `value.to_bits()` for those).
/// Returns 0 on success, -1 on error (schema missing / arity / type mismatch).
/// # Safety
/// `out` must be writable for `out_cap` bytes; `out_len` writable.

/// Total decoded-state bytes currently held (diagnostics).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_state_ready(handle: u32) -> i32 {
    with_core(handle, |core| if core.state().ready { 1 } else { 0 }).unwrap_or(-1)
}

// --- schema metadata (bindings build their language mirror from these) ----

fn write_str(out: *mut u8, cap: usize, s: &str) -> i32 {
    let bytes = s.as_bytes();
    let n = bytes.len().min(cap);
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, n) };
    bytes.len() as i32
}

/// Schema count in the descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_count(handle: u32) -> u32 {
    with_core(handle, |core| core.schema_count() as u32).unwrap_or(0)
}

/// Schema name (writes into `out`, returns full length, -1 = bad handle/index).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_name(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.schema_at(index as usize).map(|s| write_str(out, cap, &s.name))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Schema wire hash (same algorithm the sections carry).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_hash(handle: u32, index: u32) -> u32 {
    with_core(handle, |core| {
        core.schema_at(index as usize).map(engine_schema::wire_hash)
    }).and_then(|r| r).unwrap_or(0)
}

/// Schema version.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_version(handle: u32, index: u32) -> u32 {
    with_core(handle, |core| {
        core.schema_at(index as usize).map(|s| s.version)
    }).and_then(|r| r).unwrap_or(0)
}

/// Field count of one schema.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_count(handle: u32, index: u32) -> u32 {
    with_core(handle, |core| {
        core.schema_at(index as usize).map(|s| s.fields.len() as u32)
    }).and_then(|r| r).unwrap_or(0)
}

/// Field name of one schema field.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_name(handle: u32, index: u32, field: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.schema_at(index as usize)
            .and_then(|s| s.fields.get(field as usize))
            .map(|f| write_str(out, cap, &f.name))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Flattened nested path of one schema field (`stats` for `stats__kills`),
/// or -1 when the field is not nested. The value is a display-only grouping
/// hint: the wire still carries the flattened scalar.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_nested(handle: u32, index: u32, field: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.schema_at(index as usize)
            .and_then(|s| s.fields.get(field as usize))
            .and_then(|f| f.nested_path.as_deref())
            .map(|path| write_str(out, cap, path))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Field kind (`engine_schema::WireKind` discriminant: 0=u8, 1=i8, 2=u16,
/// 3=i16, 4=u32, 5=i32, 6=f32, 7=f64, 8=bool, 9=i64, 10=string, 11=array).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_kind(handle: u32, index: u32, field: u32) -> i32 {
    with_core(handle, |core| {
        core.schema_at(index as usize)
            .and_then(|s| s.fields.get(field as usize))
            .map(|f| f.kind as u32 as i32)
    }).and_then(|r| r).unwrap_or(-1)
}

/// Field flags (bit 1 = optional, 2 = static, 4 = transient).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_flags(handle: u32, index: u32, field: u32) -> i32 {
    with_core(handle, |core| {
        core.schema_at(index as usize)
            .and_then(|s| s.fields.get(field as usize))
            .map(|f| f.flags as i32)
    }).and_then(|r| r).unwrap_or(-1)
}

/// Array element kind (0 = not an array).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_schema_field_element_kind(handle: u32, index: u32, field: u32) -> i32 {
    with_core(handle, |core| {
        core.schema_at(index as usize)
            .and_then(|s| s.fields.get(field as usize))
            .map(|f| f.element.map(|e| e as u32 as i32).unwrap_or(0))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Number of nested-array collections lowered from `t.array(Item)`.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_nested_count(handle: u32) -> u32 {
    with_core(handle, |core| core.descriptor().root.nested.len() as u32).unwrap_or(0)
}

fn nested_part(handle: u32, index: u32, part: fn(&engine_schema::NestedCollection) -> &str) -> Option<String> {
    with_core(handle, |core| {
        core.descriptor().root.nested.get(index as usize).map(|n| part(n).to_string())
    }).flatten()
}

/// Child collection name of nested entry `index`.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_nested_collection(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    match nested_part(handle, index, |n| &n.collection) {
        Some(value) => write_str(out, cap, &value),
        None => -1,
    }
}

/// Parent collection name of nested entry `index`.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_nested_parent(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    match nested_part(handle, index, |n| &n.parent) {
        Some(value) => write_str(out, cap, &value),
        None => -1,
    }
}

/// Field name on the parent entity (`items` for `parent.items`).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_nested_field(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    match nested_part(handle, index, |n| &n.field) {
        Some(value) => write_str(out, cap, &value),
        None => -1,
    }
}

/// Collection count in the room state.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_collection_count(handle: u32) -> u32 {
    with_core(handle, |core| core.collection_count() as u32).unwrap_or(0)
}

/// Collection schema wire hash (the hash collection rows carry).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_collection_schema_hash(handle: u32, index: u32) -> u32 {
    with_core(handle, |core| {
        core.collection_at(index as usize).map(|c| c.schema_hash)
    }).and_then(|r| r).unwrap_or(0)
}

/// Collection name by position.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_collection_name(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.collection_at(index as usize).map(|c| write_str(out, cap, &c.name))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Entity count in collection `ci`.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_entity_count_in(handle: u32, ci: u32) -> u32 {
    with_core(handle, |core| {
        core.collection_at(ci as usize).map(|c| c.entities.len() as u32)
    }).and_then(|r| r).unwrap_or(0)
}

/// Entity id at position `pos` in collection `ci` (u32::MAX = out of range).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_entity_id_at_in(handle: u32, ci: u32, pos: u32) -> u32 {
    with_core(handle, |core| {
        core.collection_at(ci as usize)
            .and_then(|c| c.entities.keys().nth(pos as usize).copied())
    }).and_then(|r| r).unwrap_or(u32::MAX)
}

/// i64 field of one entity (0 when absent/not i64).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_i64_by_index(handle: u32, entity_id: u32, index: usize) -> i64 {
    with_core(handle, |core| {
        let state = core.state();
        let coll = state.collections.first()?;
        let entity = coll.entities.get(&entity_id)?;
        entity.values.get(index).and_then(|v| v.get_i64())
    })
    .and_then(|r| r)
    .unwrap_or(0)
}

/// f64 read of one entity field, in collection `ci` (NaN = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_f64_in(handle: u32, ci: u32, entity_id: u32, index: usize) -> f64 {
    with_core(handle, |core| {
        core.collection_at(ci as usize)?
            .entities.get(&entity_id)?
            .values.get(index)
            .and_then(|v| v.get_f64())
    })
    .and_then(|r| r)
    .unwrap_or(f64::NAN)
}

/// String read of one entity field, in collection `ci` (-1 = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_string_in(handle: u32, ci: u32, entity_id: u32, index: usize, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.collection_at(ci as usize)?
            .entities.get(&entity_id)?
            .values.get(index)
            .and_then(|v| v.get_str())
            .map(|s| write_str(out, cap, s))
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

/// Bool read of one entity field, in collection `ci` (-1 = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_bool_in(handle: u32, ci: u32, entity_id: u32, index: usize) -> i32 {
    with_core(handle, |core| {
        core.collection_at(ci as usize)?
            .entities.get(&entity_id)?
            .values.get(index)
            .and_then(|v| v.get_bool())
            .map(|b| if b { 1 } else { 0 })
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

// --- root scalars (kind-3/4 world frames) ---------------------------------

/// Root scalar field count.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_root_field_count(handle: u32) -> u32 {
    with_core(handle, |core| {
        Some(core.descriptor().root.scalars.len() as u32)
    }).and_then(|r| r).unwrap_or(0)
}

/// Root scalar field name.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_root_field_name(handle: u32, index: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.descriptor().root.scalars.get(index as usize)
            .map(|f| write_str(out, cap, &f.name))
    }).and_then(|r| r).unwrap_or(-1)
}

/// Root scalar field kind (`WireKind` discriminant).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_root_field_kind(handle: u32, index: u32) -> i32 {
    with_core(handle, |core| {
        core.descriptor().root.scalars.get(index as usize)
            .map(|f| f.kind as u32 as i32)
    }).and_then(|r| r).unwrap_or(-1)
}

/// Root scalar schema name.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_root_schema_name(handle: u32, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| Some(write_str(out, cap, &core.descriptor().root.name)))
    .and_then(|r| r).unwrap_or(-1)
}

/// Root scalar schema version.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_root_schema_version(handle: u32) -> u32 {
    with_core(handle, |core| Some(core.descriptor().root.version))
    .and_then(|r| r).unwrap_or(0)
}

/// f64 read of one root scalar (NaN = absent / not numeric).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_root_f64(handle: u32, index: usize) -> f64 {
    with_core(handle, |core| {
        core.state().root_value(index).and_then(|v| v.get_f64())
    })
    .and_then(|r| r)
    .unwrap_or(f64::NAN)
}

/// String read of one root scalar (-1 = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_root_string(handle: u32, index: usize, out: *mut u8, cap: usize) -> i32 {
    if out.is_null() { return -1; }
    with_core(handle, |core| {
        core.state().root_value(index)
            .and_then(|v| v.get_str())
            .map(|s| write_str(out, cap, s))
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

/// Bool read of one root scalar (-1 = absent).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_get_root_bool(handle: u32, index: usize) -> i32 {
    with_core(handle, |core| {
        core.state().root_value(index).and_then(|v| v.get_bool())
            .map(|b| if b { 1 } else { 0 })
    })
    .and_then(|r| r)
    .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_apply_read_roundtrip() {
        use engine_schema::{emit_descriptor, SchemaDef, SchemaField, RootState, RootCollection, WireKind};
        let player = SchemaDef {
            nested_arrays: Vec::new(),
            name: "Player".into(),
            version: 1,
            fields: vec![
                SchemaField {
                    name: "x".into(), kind: WireKind::F64, element: None,
                    ref_target: None, flags: 0, source_type: String::new(),
                    default: None, encoding: engine_schema::default_encoding(WireKind::F64),
                    nested_path: None, line: 0, column: 0,
                },
                SchemaField {
                    name: "y".into(), kind: WireKind::F64, element: None,
                    ref_target: None, flags: 0, source_type: String::new(),
                    default: None, encoding: engine_schema::default_encoding(WireKind::F64),
                    nested_path: None, line: 0, column: 0,
                },
            ],
            line: 0,
            column: 0,
        };
        let root = RootState {
            nested: Vec::new(),
            name: "BenchState".into(),
            version: 1,
            scalars: vec![],
            collections: vec![RootCollection { name: "players".into(), schema: "Player".into() }],
        };
        let packed = engine_webrtc_protocol::packed::PackedSchema::from_def(&player);
        let descriptor = emit_descriptor(&[player], Some(&root));

        let handle = unsafe { cyg_create(descriptor.as_ptr(), descriptor.len()) };
        assert!(handle != 0);

        use engine_webrtc_protocol::packed::{WireValue, encode_full};
        let mut clamped = 0;
        let section = encode_full(
            &packed,
            &[(7, vec![WireValue::F64(123.5), WireValue::F64(-4.25)])],
            &mut clamped,
        )
        .expect("packed full");

        // The payload travels as [your_id i64][section][acked u16][tick u32]
        // (the envelope cyg_apply_payload unwraps).
        let mut payload = vec![0xA5u8];
        payload.extend_from_slice(b"delta");
        payload.extend_from_slice(&1i64.to_le_bytes());
        payload.extend_from_slice(&section);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&60u32.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());

        let events = unsafe { cyg_apply_payload(handle, payload.as_ptr(), payload.len()) };
        assert_eq!(events, 2); // Ready + Add (first entity of the full snapshot)
        assert_eq!(cyg_state_ready(handle), 1);
        assert_eq!(cyg_entity_count(handle), 1);

        let cname = std::ffi::CString::new("x").unwrap();
        assert_eq!(unsafe { cyg_get_f64(handle, 7, cname.as_ptr()) }, 123.5);
        let cy = std::ffi::CString::new("y").unwrap();
        assert_eq!(unsafe { cyg_get_f64(handle, 7, cy.as_ptr()) }, -4.25);

        cyg_free(handle);
        assert_eq!(cyg_entity_count(handle), 0); // freed handle reads 0
    }
}

// --- predictor slots --------------------------------------------------------

struct PredSlots {
    preds: Vec<Option<crate::predict::Predictor>>,
}

impl PredSlots {
    fn get() -> &'static std::sync::Mutex<PredSlots> {
        static SLOTS: std::sync::OnceLock<std::sync::Mutex<PredSlots>> = std::sync::OnceLock::new();
        SLOTS.get_or_init(|| std::sync::Mutex::new(PredSlots { preds: Vec::new() }))
    }
}

fn with_pred<R>(pred_handle: u32, f: impl FnOnce(&mut crate::predict::Predictor) -> R) -> Option<R> {
    let mut slots = PredSlots::get().lock().unwrap();
    slots.preds.get_mut(pred_handle as usize - 1)?.as_mut().map(f)
}

fn with_pred_and_core<R>(
    pred_handle: u32,
    core_handle: u32,
    f: impl FnOnce(&mut crate::predict::Predictor, &mut ClientCore) -> R,
) -> Option<R> {
    let mut cores = Slots::get().lock().unwrap();
    let core = cores.cores.get_mut(core_handle as usize - 1)?.as_mut()?;
    with_pred(pred_handle, |pred| f(pred, core))
}

// --- prediction / rollback ABI ----------------------------------------------

/// Create a predictor bound to `collection_slot` of the core's state.
/// Returns a predictor handle (0 = bad core handle / slot).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_create(core_handle: u32, collection_slot: u32) -> u32 {
    let ok = with_core(core_handle, |core| collection_slot < core.collection_count() as u32);
    if !ok.unwrap_or(false) {
        return 0;
    }
    let mut slots = PredSlots::get().lock().unwrap();
    slots.preds.push(Some(crate::predict::Predictor::new(collection_slot as usize)));
    slots.preds.len() as u32
}

/// Release a predictor handle.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_free(pred_handle: u32) {
    let mut slots = PredSlots::get().lock().unwrap();
    let idx = (pred_handle as usize).checked_sub(1);
    if let Some(slot) = idx.and_then(|i| slots.preds.get_mut(i)) {
        *slot = None;
    }
}

/// Snapshot every entity of the tracked collection as the pre-prediction
/// rollback baseline. Returns rows captured, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_capture(pred_handle: u32, core_handle: u32) -> i32 {
    with_pred_and_core(pred_handle, core_handle, |pred, core| {
        pred.capture_pre_state(core.state()) as i32
    })
    .unwrap_or(-1)
}

/// Restore the rollback baseline into the state. Returns rows restored, -1.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_restore(pred_handle: u32, core_handle: u32) -> i32 {
    with_pred_and_core(pred_handle, core_handle, |pred, core| {
        pred.restore_rollback(core.state_mut()) as i32
    })
    .unwrap_or(-1)
}

/// Buffer one predicted input (raw i64s, same encoding as
/// `cyg_encode_schema_input`: floats carried as bit patterns). The game
/// applies it to the entity itself, then calls `cyg_pred_record`.
/// Returns 0, or -1 (bad handle / out-of-order seq).
/// # Safety
/// `values` must point to `len` readable i64s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_pred_predict(
    pred_handle: u32,
    seq: u16,
    values: *const i64,
    len: usize,
) -> i32 {
    if values.is_null() {
        return -1;
    }
    let raw = unsafe { std::slice::from_raw_parts(values, len) };
    let ok = with_pred(pred_handle, |pred| pred.predict(seq, raw.to_vec()).is_ok());
    if ok.unwrap_or(false) { 0 } else { -1 }
}

/// Record the entity's CURRENT values as the predicted state for `seq`
/// (after a live predict-apply or a replayed apply). 0 = ok, -1 = error.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_record(pred_handle: u32, core_handle: u32, entity_id: u32, seq: u16) -> i32 {
    with_pred_and_core(pred_handle, core_handle, |pred, core| {
        let state = core.state();
        pred.record_history(state, entity_id, seq).is_ok()
    })
    .map(|ok| if ok { 0 } else { -1 })
    .unwrap_or(-1)
}

/// One reconcile step against the core's LAST acked seq
/// (`cyg_last_acked_seq`): measure divergence at the ack, drop acked
/// pending inputs, ADOPT the authoritative values into the entity.
/// Returns the number of pending inputs the game must replay (peek each
/// with `cyg_pred_pending_seq` / `cyg_pred_pop_input`, apply, then
/// `cyg_pred_record`), or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_reconcile(pred_handle: u32, core_handle: u32, entity_id: u32) -> i32 {
    with_pred_and_core(pred_handle, core_handle, |pred, core| {
        let acked = core.last_acked_seq();
        let outcome = pred.reconcile(core.state_mut(), entity_id, acked);
        outcome.replay_count as i32
    })
    .unwrap_or(-1)
}

/// Pending (unacknowledged) input count.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_pending_count(pred_handle: u32) -> u32 {
    with_pred(pred_handle, |pred| pred.pending().len() as u32).unwrap_or(0)
}

/// Seq of the pending input at logical position `pos` (u32::MAX = out of
/// range).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_pred_pending_seq(pred_handle: u32, pos: u32) -> u32 {
    with_pred(pred_handle, |pred| {
        pred.pending().get(pos as usize).map(|p| p.seq as u32)
    })
    .flatten()
    .unwrap_or(u32::MAX)
}

/// Copy the pending input at `pos` into `out` (PEEK — the queue drains only
/// via acks; a replayed input stays for the next reconcile). Returns 0, -1
/// on error, -2 when `cap` is too small.
/// # Safety
/// `out`/`out_len` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_pred_pop_input(
    pred_handle: u32,
    pos: u32,
    out: *mut i64,
    cap: usize,
    out_len: *mut usize,
) -> i32 {
    if out.is_null() || out_len.is_null() {
        return -1;
    }
    let values = match with_pred(pred_handle, |pred| {
        pred.pending_values(pos as usize).map(<[i64]>::to_vec)
    })
    .flatten()
    {
        Some(v) => v,
        None => return -1,
    };
    if values.len() > cap {
        return -2;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
        *out_len = values.len();
    }
    0
}

/// Divergence / drift stats into `out` (7 f64s):
/// `[last_max_drift, ema, peak, confirmed, predicted, acked_total, mismatch_events]`.
/// Returns 7, or -1 (bad handle / null).
/// # Safety
/// `out` must be writable for 7 f64s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_pred_divergence(pred_handle: u32, out: *mut f64) -> i32 {
    if out.is_null() {
        return -1;
    }
    match with_pred(pred_handle, |pred| {
        let s = pred.stats();
        [
            s.last_max_drift,
            s.ema,
            s.peak,
            s.confirmed as f64,
            s.predicted as f64,
            s.acked_total as f64,
            s.mismatch_events as f64,
        ]
    }) {
        Some(vals) => {
            unsafe {
                for (i, v) in vals.iter().enumerate() {
                    *out.add(i) = *v;
                }
            }
            vals.len() as i32
        }
        None => -1,
    }
}

/// Acked input seq carried by the last applied payload (the prediction ack).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_last_acked_seq(core_handle: u32) -> u32 {
    with_core(core_handle, |core| core.last_acked_seq() as u32).unwrap_or(0)
}

/// Server simulation tick carried by the last applied payload.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_last_server_tick(core_handle: u32) -> u32 {
    with_core(core_handle, |core| core.last_server_tick()).unwrap_or(0)
}

/// The tick a renderer should sample at (`newest_tick - delay_ticks`), or 0 before any state.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_render_tick(core_handle: u32) -> u32 {
    with_core(core_handle, |core| core.render_tick().unwrap_or(0)).unwrap_or(0)
}

/// Render delay in ticks (default 2).
#[unsafe(no_mangle)]
pub extern "C" fn cyg_interp_delay(core_handle: u32) -> u32 {
    with_core(core_handle, |core| core.interp().delay_ticks()).unwrap_or(0)
}

/// Sets the render delay in ticks.
#[unsafe(no_mangle)]
pub extern "C" fn cyg_set_interp_delay(core_handle: u32, ticks: u32) {
    let _ = with_core(core_handle, |core| core.interp_mut().set_delay_ticks(ticks));
}

/// Entity ids in the newest snapshot, written to `out`. Returns how many were written, or -1 when
/// the handle is unknown.
/// # Safety
/// `out` must point to `cap` writable `u32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_interp_entities(core_handle: u32, out: *mut u32, cap: usize) -> i32 {
    if out.is_null() {
        return -1;
    }
    match with_core(core_handle, |core| core.interp().entities()) {
        Some(ids) => {
            let n = ids.len().min(cap);
            unsafe {
                for (i, id) in ids.iter().take(n).enumerate() {
                    *out.add(i) = *id;
                }
            }
            n as i32
        }
        None => -1,
    }
}

/// Interpolated values of `entity_id` at the current render tick, written to `out`.
///
/// One value per schema field, in field order; a field that is not numeric comes back as NaN.
/// Returns the number of values written, or 0 when the entity is not in the ring.
/// # Safety
/// `out` must point to `cap` writable `f32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_interp_sample(
    core_handle: u32,
    entity_id: u32,
    out: *mut f32,
    cap: usize,
) -> usize {
    if out.is_null() {
        return 0;
    }
    let mut values: Vec<f32> = Vec::new();
    let sampled = with_core(core_handle, |core| core.sample_render(entity_id, &mut values))
        .unwrap_or(false);
    if !sampled {
        return 0;
    }
    let n = values.len().min(cap);
    unsafe {
        for (i, v) in values.iter().take(n).enumerate() {
            *out.add(i) = *v;
        }
    }
    n
}

/// Interpolation counters, in order: snapshots, blended, snapped, clamped, held, jumps,
/// despawns, and the largest observed step. Returns 8, or 0 for an unknown handle.
/// # Safety
/// `out` must point to 8 writable `f64`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cyg_interp_stats(core_handle: u32, out: *mut f64) -> usize {
    if out.is_null() {
        return 0;
    }
    let Some(stats) = with_core(core_handle, |core| core.interp().stats()) else {
        return 0;
    };
    let values = [
        stats.snapshots as f64,
        stats.blended as f64,
        stats.snapped as f64,
        stats.clamped as f64,
        stats.held as f64,
        stats.jumps as f64,
        stats.despawns as f64,
        stats.max_step as f64,
    ];
    unsafe {
        for (i, v) in values.iter().enumerate() {
            *out.add(i) = *v;
        }
    }
    values.len()
}

#[cfg(test)]
mod pred_tests {
    use super::*;
    use engine_schema::{emit_descriptor, RootCollection, RootState, SchemaDef, SchemaField, WireKind};

    /// Player schema [name Str, x F64, y F64, alive Bool]; input schema
    /// [dir I32, speed F64] — the loop drives prediction purely over the ABI.
    fn descriptor() -> Vec<u8> {
        let f = |name: &str, kind: WireKind| SchemaField {
            name: name.into(),
            kind,
            element: None,
            ref_target: None,
            flags: 0,
            source_type: String::new(),
            default: None,
            encoding: engine_schema::default_encoding(kind),
            nested_path: None,
            line: 0,
            column: 0,
        };
        let player = player_schema();
        let input = SchemaDef {
            nested_arrays: Vec::new(),
            name: "Input".into(),
            version: 1,
            fields: vec![f("dir", WireKind::I32), f("speed", WireKind::F64)],
            line: 0, column: 0,
        };
        let root = RootState {
            nested: Vec::new(),
            name: "Arena".into(),
            version: 1,
            scalars: vec![],
            collections: vec![RootCollection { name: "players".into(), schema: "Player".into() }],
        };
        emit_descriptor(&[player, input], Some(&root))
    }

    fn player_schema() -> SchemaDef {
        let f = |name: &str, kind: WireKind| SchemaField {
            name: name.into(),
            kind,
            element: None,
            ref_target: None,
            flags: 0,
            source_type: String::new(),
            default: None,
            encoding: engine_schema::default_encoding(kind),
            nested_path: None,
            line: 0,
            column: 0,
        };
        SchemaDef {
            nested_arrays: Vec::new(),
            name: "Player".into(),
            version: 1,
            fields: vec![f("name", WireKind::String), f("x", WireKind::F64), f("y", WireKind::F64), f("alive", WireKind::Bool)],
            line: 0, column: 0,
        }
    }

    fn update_section(packed: &engine_webrtc_protocol::packed::PackedSchema, x: f64) -> Vec<u8> {
        use engine_webrtc_protocol::packed::{PackedOp, WireValue, encode_patch};
        let mut clamped = 0;
        encode_patch(
            packed,
            &[PackedOp::Update {
                id: 7,
                mask: 0b0010,
                values: vec![(1, WireValue::F64(x))],
            }],
            &mut clamped,
        )
        .expect("packed patch")
    }

    fn envelope(section: &[u8], acked: u16) -> Vec<u8> {
        let mut p = vec![0xA5u8];
        p.extend_from_slice(b"delta");
        p.extend_from_slice(&1i64.to_le_bytes());
        p.extend_from_slice(section);
        p.extend_from_slice(&acked.to_le_bytes());
        p.extend_from_slice(&10u32.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes()); // client stamp
        p
    }

    #[test]
    fn prediction_loop_over_the_abi() {
        let desc = descriptor();
        let handle = unsafe { cyg_create(desc.as_ptr(), desc.len()) };
        assert!(handle != 0);

        // full snapshot with x = 100
        use engine_webrtc_protocol::packed::{WireValue, encode_full};
        let packed = engine_webrtc_protocol::packed::PackedSchema::from_def(&player_schema());
        let mut clamped = 0;
        let full = encode_full(
            &packed,
            &[(
                7,
                vec![
                    WireValue::Str("me".into()),
                    WireValue::F64(100.0),
                    WireValue::F64(0.0),
                    WireValue::Bool(true),
                ],
            )],
            &mut clamped,
        )
        .expect("packed full");
        let mut payload = vec![0xA5u8];
        payload.extend_from_slice(b"delta");
        payload.extend_from_slice(&1i64.to_le_bytes());
        payload.extend_from_slice(&full);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&0u16.to_le_bytes());
        assert!(unsafe { cyg_apply_payload(handle, payload.as_ptr(), payload.len()) } >= 0);

        let pred = cyg_pred_create(handle, 0);
        assert!(pred != 0);
        assert_eq!(cyg_pred_capture(pred, handle), 1);
        assert_eq!(cyg_last_acked_seq(handle), 0);
        assert_eq!(cyg_last_server_tick(handle), 1);

        // predict seq 1: dir=2 speed=3 → x = 106 (game applies locally and records)
        let mut values = [2i64, 3.0f64.to_bits() as i64];
        assert_eq!(unsafe { cyg_pred_predict(pred, 1, values.as_ptr(), 2) }, 0);
        // out-of-order predict rejected over the ABI too
        assert_eq!(unsafe { cyg_pred_predict(pred, 0, values.as_ptr(), 2) }, -1);
        assert_eq!(cyg_pred_pending_count(pred), 1);
        assert_eq!(cyg_pred_pending_seq(pred, 0), 1);

        // sanity: the same raw-i64s encode cleanly onto the input wire
        let mut out = [0u8; 64];
        let mut out_len = 0usize;
        assert_eq!(unsafe {
            cyg_encode_schema_input(handle, 1, values.as_ptr(), 2, out.as_mut_ptr(), out.len(), &mut out_len)
        }, 0);
        // the game would now apply dir*speed to its local entity (native
        // code the core can't see) and record the post-apply values; this
        // test records the pre-apply state (x = 100), so the reconcile
        // below must report drift |100 − 106| = 6 — exactly what a
        // mispredicting applier produces.
        assert_eq!(cyg_pred_record(pred, handle, 7, 1), 0);

        // server truth at acked 1 = 106 → divergence |100 − 106| = 6
        let section = update_section(&packed, 106.0);
        let payload = envelope(&section, 1);
        assert!(unsafe { cyg_apply_payload(handle, payload.as_ptr(), payload.len()) } >= 0);
        assert_eq!(cyg_last_acked_seq(handle), 1);
        assert_eq!(cyg_pred_reconcile(pred, handle, 7), 0); // nothing left to replay

        let mut stats = [0f64; 7];
        assert_eq!(unsafe { cyg_pred_divergence(pred, stats.as_mut_ptr()) }, 7);
        assert_eq!(stats[3], 1.0); // confirmed
        assert_eq!(stats[4], 1.0); // predicted
        assert_eq!(stats[5], 1.0); // acked_total
        assert!((stats[0] - 6.0).abs() < 1e-9, "drift = {}", stats[0]);

        // values-out path
        let mut vals = [0i64; 4];
        let mut n = 0usize;
        // pending is empty now → -1
        assert_eq!(unsafe { cyg_pred_pop_input(pred, 0, vals.as_mut_ptr(), 4, &mut n) }, -1);

        cyg_pred_free(pred);
        assert_eq!(cyg_pred_pending_count(pred), 0);
        cyg_free(handle);
    }
}
