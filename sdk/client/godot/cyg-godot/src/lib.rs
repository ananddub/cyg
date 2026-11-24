//! Godot GDExtension over cyg-core-rs.
//!
//! GDScript usage:
//!   var room := CygRoom.new()
//!   room.load_descriptor(descriptor_bytes)      # from cyg.schema.bin
//!   room.apply_payload(frame_bytes)             # from WebSocketPeer packets
//!   var x := room.get_f64(entity_id, room.field_index("x"))
//!   var input := room.encode_schema_input(seq, packed_i64_array)
//!
//! The transport stays in GDScript (WebSocketPeer); all protocol logic is
//! cyg-core-rs — the same core the web/Unity/Unreal SDKs use.

use godot::prelude::*;

use cyg_core_rs::input::InputValue;
use cyg_core_rs::state::Value;
use cyg_core_rs::{ClientCore, WireKind};

struct CygExtension;

#[gdextension]
unsafe impl ExtensionLibrary for CygExtension {}

#[derive(GodotClass)]
#[class(base = RefCounted)]
/// Server-authoritative room state driven by cyg-core-rs.
struct CygRoom {
    /// None until a valid descriptor has been loaded (CygRoom.new() has no
    /// descriptor bytes yet).
    core: Option<ClientCore>,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for CygRoom {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            core: None,
            base,
        }
    }
}

/// Map one raw i64 (GDScript int, or f32/f64 bit pattern) to the typed input
/// value the schema field kind expects.
fn input_value_from_i64(raw: i64, kind: WireKind) -> InputValue {
    match kind {
        WireKind::U8 => InputValue::U8(raw as u8),
        WireKind::U16 => InputValue::U16(raw as u16),
        WireKind::U32 => InputValue::U32(raw as u32),
        WireKind::I8 => InputValue::I8(raw as i8),
        WireKind::I16 => InputValue::I16(raw as i16),
        WireKind::I32 => InputValue::I32(raw as i32),
        WireKind::I64 => InputValue::I64(raw),
        WireKind::F32 => InputValue::F32(f32::from_bits(raw as u32)),
        WireKind::F64 => InputValue::F64(f64::from_bits(raw as u64)),
        WireKind::Bool => InputValue::Bool(raw != 0),
        _ => InputValue::I64(raw),
    }
}

#[godot_api]
impl CygRoom {
    /// Load/replace the schema descriptor. Returns false on a bad descriptor.
    #[func]
    fn load_descriptor(&mut self, bytes: PackedByteArray) -> bool {
        match ClientCore::new(bytes.to_vec().as_slice()) {
            Ok(core) => {
                self.core = Some(core);
                true
            }
            Err(_) => {
                self.core = None;
                false
            }
        }
    }

    /// Apply one raw room-data payload; returns the number of events, or -1
    /// on a decode error (non-delta frames are skipped by the core).
    #[func]
    fn apply_payload(&mut self, payload: PackedByteArray) -> i32 {
        let core = match self.core.as_mut() {
            Some(c) => c,
            None => return -1,
        };
        match core.apply_payload(payload.as_slice()) {
            Ok(events) => events.len() as i32,
            Err(_) => -1,
        }
    }

    /// Entity count in the primary collection.
    #[func]
    fn entity_count(&self) -> i64 {
        self.core
            .as_ref()
            .and_then(|c| c.state().collections.first())
            .map(|col| col.entities.len() as i64)
            .unwrap_or(0)
    }

    /// Entity id at logical position `pos` (ascending id order); -1 = none.
    #[func]
    fn entity_id_at(&self, pos: i64) -> i64 {
        if pos < 0 {
            return -1;
        }
        self.core
            .as_ref()
            .and_then(|c| c.state().collections.first())
            .and_then(|col| col.entities.keys().nth(pos as usize).copied())
            .map(|id| id as i64)
            .unwrap_or(-1)
    }

    /// Schema-field index by name; -1 = not found.
    #[func]
    fn field_index(&self, name: GString) -> i32 {
        let name = name.to_string();
        self.core.as_ref().and_then(|core| {
            core.state()
                .collections
                .first()
                .and_then(|coll| {
                    core.descriptor()
                        .schemas
                        .iter()
                        .find(|s| cyg_core_rs::wire_hash(s) == coll.schema_hash)
                        .and_then(|s| s.fields.iter().position(|f| f.name == name))
                        .map(|i| i as i32)
                })
        }).unwrap_or(-1)
    }

    /// f64 view of one entity field by index; NaN = absent.
    #[func]
    fn get_f64(&self, entity_id: i64, index: i32) -> f64 {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return f64::NAN;
        }
        self.core
            .as_ref()
            .and_then(|c| c.state().collections.first())
            .and_then(|col| col.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .map(|v| v.as_f64())
            .unwrap_or(f64::NAN)
    }

    /// String field by index; empty string = absent/not a string.
    #[func]
    fn get_string(&self, entity_id: i64, index: i32) -> GString {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return GString::new();
        }
        self.core
            .as_ref()
            .and_then(|c| c.state().collections.first())
            .and_then(|col| col.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .and_then(|v| match v {
                Value::Str(s) => Some(GString::from(s.as_str())),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Bool field by index; false = absent/not a bool (use get_f64 for
    /// presence checks on numeric fields).
    #[func]
    fn get_bool(&self, entity_id: i64, index: i32) -> bool {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return false;
        }
        self.core
            .as_ref()
            .and_then(|c| c.state().collections.first())
            .and_then(|col| col.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .map(|v| matches!(v, Value::Bool(true)))
            .unwrap_or(false)
    }

    /// Client ready (kind-1 full snapshot applied)?
    #[func]
    fn ready(&self) -> bool {
        self.core
            .as_ref()
            .map(|c| c.state().ready)
            .unwrap_or(false)
    }

    // --- schema metadata (GDScript mirrors are built from these — no
    // second descriptor parser lives in the addon) --------------------------

    fn schema_at(&self, index: i64) -> Option<&cyg_core_rs::SchemaDef> {
        if index < 0 { return None; }
        self.core.as_ref().and_then(|c| c.schema_at(index as usize))
    }

    /// Schema count in the parsed descriptor.
    #[func]
    fn schema_count(&self) -> i64 {
        self.core.as_ref().map(|c| c.schema_count() as i64).unwrap_or(0)
    }

    #[func]
    fn schema_name(&self, index: i64) -> GString {
        self.schema_at(index).map(|s| GString::from(s.name.as_str())).unwrap_or_default()
    }

    #[func]
    fn schema_hash(&self, index: i64) -> i64 {
        self.schema_at(index)
            .map(|s| cyg_core_rs::wire_hash(s) as i64)
            .unwrap_or(0)
    }

    #[func]
    fn schema_version(&self, index: i64) -> i64 {
        self.schema_at(index).map(|s| s.version as i64).unwrap_or(0)
    }

    #[func]
    fn schema_field_count(&self, index: i64) -> i64 {
        self.schema_at(index).map(|s| s.fields.len() as i64).unwrap_or(0)
    }

    #[func]
    fn schema_field_name(&self, index: i64, field: i64) -> GString {
        self.schema_at(index)
            .and_then(|s| s.fields.get(field.max(0) as usize))
            .map(|f| GString::from(f.name.as_str()))
            .unwrap_or_default()
    }

    /// Field kind discriminant: 0=u8 1=i8 2=u16 3=i16 4=u32 5=i32 6=f32
    /// 7=f64 8=bool 9=i64 10=string 11=array.
    #[func]
    fn schema_field_kind(&self, index: i64, field: i64) -> i32 {
        self.schema_at(index)
            .and_then(|s| s.fields.get(field.max(0) as usize))
            .map(|f| f.kind as u32 as i32)
            .unwrap_or(-1)
    }

    /// Field flags: 1 = optional, 2 = static, 4 = transient.
    #[func]
    fn schema_field_flags(&self, index: i64, field: i64) -> i32 {
        self.schema_at(index)
            .and_then(|s| s.fields.get(field.max(0) as usize))
            .map(|f| f.flags as i32)
            .unwrap_or(-1)
    }

    /// Array element kind (0 = not an array).
    #[func]
    fn schema_field_element_kind(&self, index: i64, field: i64) -> i32 {
        self.schema_at(index)
            .and_then(|s| s.fields.get(field.max(0) as usize))
            .map(|f| f.element.map(|e| e as u32 as i32).unwrap_or(0))
            .unwrap_or(-1)
    }

    /// Collection count in the room state.
    #[func]
    fn collection_count(&self) -> i64 {
        self.core.as_ref().map(|c| c.collection_count() as i64).unwrap_or(0)
    }

    #[func]
    fn collection_name(&self, index: i64) -> GString {
        self.core.as_ref()
            .and_then(|c| c.collection_at(index.max(0) as usize))
            .map(|c| GString::from(c.name.as_str()))
            .unwrap_or_default()
    }

    /// Wire hash of the schema a collection's rows carry.
    #[func]
    fn collection_schema_hash(&self, index: i64) -> i64 {
        self.core.as_ref()
            .and_then(|c| c.collection_at(index.max(0) as usize))
            .map(|c| c.schema_hash as i64)
            .unwrap_or(0)
    }

    /// Entity count in collection `ci`.
    #[func]
    fn entity_count_in(&self, ci: i64) -> i64 {
        self.core.as_ref()
            .and_then(|c| c.collection_at(ci.max(0) as usize))
            .map(|c| c.entities.len() as i64)
            .unwrap_or(0)
    }

    /// Entity id at position `pos` in collection `ci`; -1 = out of range.
    #[func]
    fn entity_id_at_in(&self, ci: i64, pos: i64) -> i64 {
        if pos < 0 { return -1; }
        self.core.as_ref()
            .and_then(|c| c.collection_at(ci.max(0) as usize))
            .and_then(|c| c.entities.keys().nth(pos as usize).copied())
            .map(|id| id as i64)
            .unwrap_or(-1)
    }

    /// f64 read of one entity field in collection `ci`; NaN = absent.
    #[func]
    fn get_f64_in(&self, ci: i64, entity_id: i64, index: i64) -> f64 {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return f64::NAN;
        }
        self.core.as_ref()
            .and_then(|c| c.collection_at(ci.max(0) as usize))
            .and_then(|c| c.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .and_then(|v| v.get_f64())
            .unwrap_or(f64::NAN)
    }

    /// String read of one entity field in collection `ci`; "" = absent.
    #[func]
    fn get_string_in(&self, ci: i64, entity_id: i64, index: i64) -> GString {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return GString::new();
        }
        self.core.as_ref()
            .and_then(|c| c.collection_at(ci.max(0) as usize))
            .and_then(|c| c.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .and_then(|v| v.get_str())
            .map(|s| GString::from(s))
            .unwrap_or_default()
    }

    /// Bool read of one entity field in collection `ci`; -1 = absent.
    #[func]
    fn get_bool_in(&self, ci: i64, entity_id: i64, index: i64) -> i32 {
        if index < 0 || !(0..=u32::MAX as i64).contains(&entity_id) {
            return -1;
        }
        self.core.as_ref()
            .and_then(|c| c.collection_at(ci.max(0) as usize))
            .and_then(|c| c.entities.get(&(entity_id as u32)))
            .and_then(|e| e.values.get(index as usize))
            .and_then(|v| v.get_bool())
            .map(|b| if b { 1 } else { 0 })
            .unwrap_or(-1)
    }

    // --- root scalars (kind-3/4 world frames) ------------------------------

    #[func]
    fn root_field_count(&self) -> i64 {
        self.core.as_ref()
            .map(|c| c.descriptor().root.scalars.len() as i64)
            .unwrap_or(0)
    }

    #[func]
    fn root_field_name(&self, index: i64) -> GString {
        self.core.as_ref()
            .and_then(|c| c.descriptor().root.scalars.get(index.max(0) as usize))
            .map(|f| GString::from(f.name.as_str()))
            .unwrap_or_default()
    }

    #[func]
    fn root_field_kind(&self, index: i64) -> i32 {
        self.core.as_ref()
            .and_then(|c| c.descriptor().root.scalars.get(index.max(0) as usize))
            .map(|f| f.kind as u32 as i32)
            .unwrap_or(-1)
    }

    #[func]
    fn root_schema_name(&self) -> GString {
        self.core.as_ref()
            .map(|c| GString::from(c.descriptor().root.name.as_str()))
            .unwrap_or_default()
    }

    #[func]
    fn root_schema_version(&self) -> i64 {
        self.core.as_ref()
            .map(|c| c.descriptor().root.version as i64)
            .unwrap_or(0)
    }

    /// f64 read of one root scalar; NaN = absent / not numeric.
    #[func]
    fn get_root_f64(&self, index: i64) -> f64 {
        if index < 0 { return f64::NAN; }
        self.core.as_ref()
            .and_then(|c| c.state().root_value(index as usize))
            .and_then(|v| v.get_f64())
            .unwrap_or(f64::NAN)
    }

    /// String read of one root scalar; "" = absent.
    #[func]
    fn get_root_string(&self, index: i64) -> GString {
        if index < 0 { return GString::new(); }
        self.core.as_ref()
            .and_then(|c| c.state().root_value(index as usize))
            .and_then(|v| v.get_str())
            .map(|s| GString::from(s))
            .unwrap_or_default()
    }

    /// Bool read of one root scalar; -1 = absent.
    #[func]
    fn get_root_bool(&self, index: i64) -> i32 {
        if index < 0 { return -1; }
        self.core.as_ref()
            .and_then(|c| c.state().root_value(index as usize))
            .and_then(|v| v.get_bool())
            .map(|b| if b { 1 } else { 0 })
            .unwrap_or(-1)
    }

    /// Encode a schema-lane input frame: values must match the Input schema's
    /// field order. Each entry is an i64 — integer fields pass the value,
    /// f32/f64 fields pass their IEEE bit patterns
    /// (GDScript: `f.to_bits()` / `f.to_f64_bits()`).
    #[func]
    fn encode_schema_input(&self, seq: i32, values: PackedInt64Array) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let core = match self.core.as_ref() {
            Some(c) => c,
            None => return out,
        };
        let Some(input_schema) = core
            .descriptor()
            .schemas
            .iter()
            .find(|s| s.name == cyg_core_rs::INPUT_SCHEMA_NAME)
        else {
            return out;
        };
        let kinds: Vec<WireKind> = input_schema.fields.iter().map(|f| f.kind).collect();
        let packed: Vec<InputValue> = values
            .as_slice()
            .iter()
            .enumerate()
            .map(|(i, raw)| {
                input_value_from_i64(*raw, kinds.get(i).copied().unwrap_or(WireKind::I64))
            })
            .collect();
        if let Ok(wire) = core.encode_schema_input(seq as u16, &packed) {
            for b in wire {
                out.push(b);
            }
        }
        out
    }
}
