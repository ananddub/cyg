//! Decoded client state + the OXS2 section decoder.
//!
//! Port of `sdk/client/web/cyg.js` `_readSchemaField` / `_decodeEntity` /
//! `_applySchemaFrame` / `_applyCollectionSection` / `_applyWorldSection`
//! (lines ~2600-2960). Byte-for-byte same wire semantics; the JS file stays
//! the reference and `tests/` in this crate pins the parity.

use std::collections::BTreeMap;

use engine_schema::{Descriptor, SchemaDef, SchemaField};

pub const SCHEMA_MAGIC: [u8; 4] = [0x4F, 0x58, 0x53, 0x32]; // "OXS2"

/// The only wire kinds this client speaks. The fixed-width kinds 1-4 exist
/// only between the guest wasm and the host, which transcodes them; a client
/// that receives one is talking to an old server and must rebuild.
pub const KIND_FULL: u8 = 5;
pub const KIND_PATCH: u8 = 6;
pub const KIND_WORLD_FULL: u8 = 7;
pub const KIND_WORLD_PATCH: u8 = 8;

// The client and the protocol crate must agree on these numbers; a mismatch
// would be a silent "unknown kind", so it is a compile error instead.
const _: () = assert!(KIND_FULL == engine_webrtc_protocol::packed::KIND_FULL);
const _: () = assert!(KIND_PATCH == engine_webrtc_protocol::packed::KIND_PATCH);
const _: () = assert!(KIND_WORLD_FULL == engine_webrtc_protocol::packed::KIND_WORLD_FULL);
const _: () = assert!(KIND_WORLD_PATCH == engine_webrtc_protocol::packed::KIND_WORLD_PATCH);

/// Patch row op codes (kept for the packed world decoder's documentation).
pub const OP_ADD: u8 = 1;
pub const OP_REMOVE: u8 = 2;
pub const OP_UPDATE: u8 = 3;

/// A decoded field value. Arrays nest.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<Value>),
    Null, // absent optional
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match *self {
            Value::F64(v) => v,
            Value::F32(v) => v as f64,
            Value::I64(v) => v as f64,
            Value::U32(v) => v as f64,
            Value::I32(v) => v as f64,
            _ => 0.0,
        }
    }

    pub fn as_i64(&self) -> i64 {
        match *self {
            Value::I64(v) => v,
            Value::U32(v) => v as i64,
            Value::I32(v) => v as i64,
            Value::U16(v) => v as i64,
            Value::I16(v) => v as i64,
            Value::U8(v) => v as i64,
            Value::I8(v) => v as i64,
            _ => 0,
        }
    }

    fn equals(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Arr(x), Value::Arr(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(i, j)| Value::equals(i, j))
            }
            (Value::F32(x), Value::F32(y)) => x == y,
            _ => a == b,
        }
    }

    /// Typed read helpers for the C-ABI getters: Some only when the value
    /// matches the requested kind, so bindings never guess a field type.
    pub fn get_f64(&self) -> Option<f64> {
        match *self {
            Value::F64(v) => Some(v),
            Value::F32(v) => Some(v as f64),
            Value::I64(v) => Some(v as f64),
            Value::U32(v) => Some(v as f64),
            Value::I32(v) => Some(v as f64),
            Value::U16(v) => Some(v as f64),
            Value::I16(v) => Some(v as f64),
            Value::U8(v) => Some(v as f64),
            Value::I8(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn get_i64(&self) -> Option<i64> {
        match *self {
            Value::I64(v) => Some(v),
            Value::U32(v) => Some(v as i64),
            Value::I32(v) => Some(v as i64),
            Value::U16(v) => Some(v as i64),
            Value::I16(v) => Some(v as i64),
            Value::U8(v) => Some(v as i64),
            Value::I8(v) => Some(v as i64),
            _ => None,
        }
    }

    pub fn get_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_str(&self) -> Option<&str> {
        match *self {
            Value::Str(ref s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// One decoded entity: field values in schema field order.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: u32,
    pub values: Vec<Value>,
}

/// One collection (rows keyed by entity id).
#[derive(Debug, Default)]
pub struct Collection {
    pub name: String,
    pub schema_hash: u32,
    pub ready: bool,
    pub entities: BTreeMap<u32, Entity>,
}

/// What the decoder did with a section — bindings translate these into
/// language-native events (onAdd / onRemove / onChange / onStateChange).
#[derive(Clone, Debug, PartialEq)]
pub enum StateEvent {
    /// A kind-1 full snapshot landed for a collection; the client is ready.
    Ready { collection: String },
    /// Entity added (patch ADD or full snapshot new row).
    Add { collection: String, id: u32 },
    /// Entity removed.
    Remove { collection: String, id: u32 },
    /// Fields changed: indices into the collection's schema field list.
    Change {
        collection: String,
        id: u32,
        fields: Vec<usize>,
    },
}

#[derive(Debug)]
pub struct ClientState {
    // public read access for bindings; mutation goes through apply_section
    pub ready: bool,
    pub collections: Vec<Collection>,
    /// Root scalar values (packed world frames).
    root_values: Vec<Value>,
    root_fields: Vec<SchemaField>,
    /// The synthetic root schema compiled once — same definition and hash the
    /// host uses for the world header.
    root_packed: engine_webrtc_protocol::packed::PackedSchema,
}

impl ClientState {
    /// Build per-collection storage from the descriptor. The FIRST root
    /// collection mirrors cyg.js `this._collections[0]` — single-collection
    /// packed frames target it.
    pub fn new(descriptor: &Descriptor) -> Self {
        let mut collections = Vec::new();
        for rc in &descriptor.root.collections {
            let schema = descriptor.schemas.iter().find(|s| s.name == rc.schema);
            let schema_hash = schema.map(engine_schema::wire_hash).unwrap_or(0);
            collections.push(Collection {
                name: rc.name.clone(),
                schema_hash,
                ready: false,
                entities: BTreeMap::new(),
            });
        }
        let root_def = engine_schema::SchemaDef {
            name: descriptor.root.name.clone(),
            version: descriptor.root.version,
            fields: descriptor.root.scalars.clone(),
            line: 0,
            column: 0,
        };
        Self {
            ready: false,
            root_fields: descriptor.root.scalars.clone(),
            root_packed: engine_webrtc_protocol::packed::PackedSchema::from_def(&root_def),
            root_values: vec![Value::Null; descriptor.root.scalars.len()],
            collections,
        }
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn collection(&self, name: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.name == name)
    }

    pub fn collection_mut(&mut self, name: &str) -> Option<&mut Collection> {
        self.collections.iter_mut().find(|c| c.name == name)
    }

    pub fn root_value(&self, index: usize) -> Option<&Value> {
        self.root_values.get(index)
    }

    /// Apply one raw OXS2 section (`[OXS2][kind]...`). Returns events for
    /// bindings, or an error string mirroring cyg.js's schemaerror messages.
    pub fn apply_section(
        &mut self,
        section: &[u8],
        descriptor: &Descriptor,
    ) -> Result<Vec<StateEvent>, String> {
        if section.len() < 5 {
            return Err("truncated schema section".into());
        }
        if section[..4] != SCHEMA_MAGIC {
            return Err("schema section magic mismatch".into());
        }
        let kind = section[4];
        match kind {
            KIND_FULL | KIND_PATCH => {
                let entry = self
                    .collections
                    .first_mut()
                    .ok_or("no collection in descriptor")?;
                let schema = descriptor
                    .schemas
                    .iter()
                    .find(|s| engine_schema::wire_hash(s) == entry.schema_hash)
                    .or_else(|| descriptor.schemas.first())
                    .cloned()
                    .ok_or("descriptor has no schemas")?;
                let evs = apply_packed_section(entry, &schema, section)?;
                if kind == KIND_FULL {
                    self.ready = true;
                }
                Ok(evs)
            }
            KIND_WORLD_FULL | KIND_WORLD_PATCH => {
                self.apply_packed_world(section, kind, descriptor)
            }
            other => Err(format!(
                "unsupported schema frame kind {other}; this build speaks the packed wire only (rebuild the server)"
            )),
        }
    }

    /// Applies a packed world frame: masked root scalars plus one packed section
    /// per changed collection. The nested sections are self-describing, so the
    /// same caller handles a full world and a world patch.
    fn apply_packed_world(
        &mut self,
        section: &[u8],
        kind: u8,
        descriptor: &Descriptor,
    ) -> Result<Vec<StateEvent>, String> {
        use engine_webrtc_protocol::packed;

        if kind == KIND_WORLD_PATCH && !self.ready {
            // A world patch before the baseline would be lost; the host keeps
            // sending full frames until the ack.
            return Ok(Vec::new());
        }
        let decoded =
            packed::decode_world(&self.root_packed, section).map_err(|error| error.to_string())?;

        if kind == KIND_WORLD_FULL {
            let mut values = vec![Value::Null; self.root_fields.len()];
            for (index, value) in decoded.root {
                if let Some(slot) = values.get_mut(index) {
                    *slot = value_from_wire(value);
                }
            }
            self.root_values = values;
        } else {
            for (index, value) in decoded.root {
                if let Some(slot) = self.root_values.get_mut(index) {
                    *slot = value_from_wire(value);
                }
            }
        }

        let mut events = Vec::new();
        for (id, hash, nested) in decoded.collections {
            let entry = self
                .collections
                .get_mut(id as usize)
                .ok_or_else(|| format!("unknown collection id {id}"))?;
            if entry.schema_hash != hash {
                return Err(format!(
                    "collection '{}' schema hash mismatch: server {hash:08x} client {:08x}",
                    entry.name, entry.schema_hash
                ));
            }
            let schema = descriptor
                .schemas
                .iter()
                .find(|s| engine_schema::wire_hash(s) == hash)
                .cloned()
                .ok_or("descriptor has no matching collection schema")?;
            events.extend(apply_packed_section(entry, &schema, &nested)?);
        }
        if kind == KIND_WORLD_FULL {
            self.ready = true;
        }
        Ok(events)
    }
}

/// The full-frame reconcile, shared by the legacy and the packed decoder so both produce exactly
/// the same events for the same rows.
fn reconcile_full(
    entry: &mut Collection,
    schema: &SchemaDef,
    incoming: Vec<(u32, Entity)>,
    events: &mut Vec<StateEvent>,
) {
    let incoming_ids: Vec<u32> = incoming.iter().map(|(id, _)| *id).collect();
    let removed: Vec<u32> = entry
        .entities
        .keys()
        .filter(|id| !incoming_ids.contains(id))
        .copied()
        .collect();
    for id in removed {
        entry.entities.remove(&id);
        events.push(StateEvent::Remove {
            collection: entry.name.clone(),
            id,
        });
    }
    for (id, entity) in incoming {
        let mut changed_fields = Vec::new();
        match entry.entities.get_mut(&id) {
            None => {
                entry.entities.insert(id, entity);
                events.push(StateEvent::Add {
                    collection: entry.name.clone(),
                    id,
                });
            }
            Some(existing) => {
                for f in 0..schema.fields.len() {
                    let new_v = &entity.values[f];
                    if !Value::equals(&existing.values[f], new_v) {
                        existing.values[f] = new_v.clone();
                        changed_fields.push(f);
                    }
                }
                if !changed_fields.is_empty() {
                    events.push(StateEvent::Change {
                        collection: entry.name.clone(),
                        id,
                        fields: changed_fields,
                    });
                }
            }
        }
    }
    events.push(StateEvent::Ready {
        collection: entry.name.clone(),
    });
}

fn apply_add(entry: &mut Collection, id: u32, entity: Entity, events: &mut Vec<StateEvent>) {
    entry.entities.insert(id, entity);
    events.push(StateEvent::Add {
        collection: entry.name.clone(),
        id,
    });
}

fn apply_remove(entry: &mut Collection, id: u32, events: &mut Vec<StateEvent>) {
    entry.entities.remove(&id);
    events.push(StateEvent::Remove {
        collection: entry.name.clone(),
        id,
    });
}

fn apply_update(
    entry: &mut Collection,
    id: u32,
    values: Vec<(usize, Value)>,
    events: &mut Vec<StateEvent>,
) {
    let entity_present = entry.entities.contains_key(&id);
    let mut changed_fields = Vec::new();
    for (f, value) in values {
        if let Some(existing) = entry.entities.get_mut(&id) {
            if !Value::equals(&existing.values[f], &value) {
                existing.values[f] = value;
                changed_fields.push(f);
            }
        }
    }
    if entity_present && !changed_fields.is_empty() {
        events.push(StateEvent::Change {
            collection: entry.name.clone(),
            id,
            fields: changed_fields,
        });
    }
}

/// Decodes a bit-packed section (`engine-webrtc-protocol::packed`) into the same rows and events
/// the legacy decoder produces. One schema, one quantizer, two encodings.
fn apply_packed_section(
    entry: &mut Collection,
    schema: &SchemaDef,
    section: &[u8],
) -> Result<Vec<StateEvent>, String> {
    use engine_webrtc_protocol::packed::{self, PackedOp, PackedSchema};

    let packed = PackedSchema::from_def(schema);
    let decoded = packed::decode(&packed, section).map_err(|error| error.to_string())?;
    let mut events = Vec::new();

    if decoded.kind == packed::KIND_FULL {
        let mut incoming = Vec::with_capacity(decoded.ops.len());
        for op in decoded.ops {
            let PackedOp::Add { id, values } = op else {
                return Err("a remove op inside a full packed section".into());
            };
            incoming.push((id, entity_from(id, values)));
        }
        reconcile_full(entry, schema, incoming, &mut events);
    } else {
        for op in decoded.ops {
            match op {
                PackedOp::Add { id, values } => {
                    apply_add(entry, id, entity_from(id, values), &mut events)
                }
                PackedOp::Remove { id } => apply_remove(entry, id, &mut events),
                PackedOp::Update { id, values, .. } => {
                    let values = values
                        .into_iter()
                        .map(|(field, value)| (field, value_from_wire(value)))
                        .collect();
                    apply_update(entry, id, values, &mut events);
                }
            }
        }
    }
    entry.ready = true;
    Ok(events)
}

fn entity_from(id: u32, values: Vec<engine_webrtc_protocol::packed::WireValue>) -> Entity {
    Entity {
        id,
        values: values.into_iter().map(value_from_wire).collect(),
    }
}

fn value_from_wire(value: engine_webrtc_protocol::packed::WireValue) -> Value {
    use engine_webrtc_protocol::packed::WireValue as W;
    match value {
        W::U8(v) => Value::U8(v),
        W::U16(v) => Value::U16(v),
        W::U32(v) => Value::U32(v),
        W::I8(v) => Value::I8(v),
        W::I16(v) => Value::I16(v),
        W::I32(v) => Value::I32(v),
        W::I64(v) => Value::I64(v),
        W::F32(v) => Value::F32(v),
        W::F64(v) => Value::F64(v),
        W::Bool(v) => Value::Bool(v),
        W::Str(v) => Value::Str(v),
        W::Arr(items) => Value::Arr(items.into_iter().map(value_from_wire).collect()),
        W::Null => Value::Null,
    }
}
