use std::fmt;

use crate::encoding::{FieldEncoding, default_encoding};
use crate::wire::{FIELD_OPTIONAL, FIELD_STATIC, FIELD_TRANSIENT, WireKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaField {
    pub name: String,
    pub kind: WireKind,

    pub element: Option<WireKind>,

    pub ref_target: Option<String>,

    pub flags: u8,

    pub source_type: String,

    pub default: Option<String>,

    /// How this field travels on the bit-packed lane. Defaults to the exact width of its kind, so
    /// an unannotated schema keeps its fixed-width encoding (and its wire hash).
    pub encoding: FieldEncoding,

    /// For a flattened child-schema field, the parent field name (`stats` for
    /// `stats__kills`). Codegen uses it to expose the nested accessor; the wire
    /// still carries the flattened scalar field, so every existing packed
    /// section keeps working unchanged.
    pub nested_path: Option<String>,

    pub line: usize,
    pub column: usize,
}

impl SchemaField {
    pub fn optional(&self) -> bool {
        self.flags & FIELD_OPTIONAL != 0
    }

    pub fn full_only(&self) -> bool {
        self.flags & (FIELD_STATIC | FIELD_TRANSIENT) != 0
    }

    /// Owner-only field (`.ownerOnly()`): visible only when the row id is the
    /// viewer's own id.
    pub fn private(&self) -> bool {
        self.flags & crate::wire::FIELD_PRIVATE != 0
    }

    /// True when the field travels in its default fixed-width form, i.e. the schema never asked for
    /// a custom encoding. Only non-default encodings enter the wire hash.
    pub fn encoding_is_default(&self) -> bool {
        self.encoding == default_encoding(self.kind)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootCollection {
    pub name: String,
    pub schema: String,
}

/// A nested array (`items: t.array(Item)`) lowered to a child collection.
/// The child row id packs owner and slot (`(owner << 16) | slot`), so the
/// child schema itself stays the user's `Item` schema with no hidden fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedCollection {
    pub collection: String,
    pub parent: String,
    pub field: String,
    pub schema: String,
}

/// A schema field declared as `items: t.array(Item)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedArray {
    pub field: String,
    pub schema: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootState {
    pub name: String,
    pub version: u32,
    pub scalars: Vec<SchemaField>,
    pub collections: Vec<RootCollection>,
    /// Nested arrays lowered into child collections, in parent-first order.
    pub nested: Vec<NestedCollection>,
}

pub const MAX_COLLECTIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDef {
    pub name: String,
    pub version: u32,
    pub fields: Vec<SchemaField>,
    /// Nested arrays lowered to child collections when this schema is a root
    /// collection (`items: t.array(Item)`).
    pub nested_arrays: Vec<NestedArray>,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for SchemaDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

#[derive(Debug)]
pub struct GeneratedSchema {
    pub server_ts: String,
    pub client_dts: String,

    pub descriptor: Vec<u8>,
}

pub fn generate_with_root(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    source_name: &str,
) -> GeneratedSchema {
    let source_name = source_name.replace(['\r', '\n'], " ");
    GeneratedSchema {
        server_ts: crate::emit_server(schemas, root, &source_name),
        client_dts: crate::emit_client_dts(schemas, root, &source_name),
        descriptor: crate::emit_descriptor(schemas, root),
    }
}

/// Legal domain of one per-tick input field — the host-side equivalent of
/// Colyseus' `defineInput({ sanitize })` map form. A domain CLAMPS, never
/// rejects: every decoded frame is fixed up in place before anything reads
/// it, so a `127` movement axis arrives as `1.0` and honest clients are
/// unaffected (in-range values are identity — client prediction never
/// diverges from the server's sanitized values).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputFieldDomain {
    pub min: f32,
    pub max: f32,
}

impl InputFieldDomain {
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    /// The domain clamp, NaN-safe by construction: `f32::max`/`min` return
    /// the non-NaN operand, so `NaN.max(min).min(max)` lands on the clamp
    /// floor — closing the classic `Math.min(NaN, …)` poisoning hole that
    /// hand-rolled clamps share.
    pub fn clamp(self, v: f32) -> f32 {
        debug_assert!(self.min <= self.max, "inverted input domain");
        v.max(self.min).min(self.max)
    }
}

/// Per-field domains of the room's `Input` schema: field name -> domain.
/// Optional today — when a room declares no domains, the host falls back to
/// the standard frame's built-in domains (see `netcode::standard_input_domains`).
pub type InputDomainMap = std::collections::BTreeMap<String, InputFieldDomain>;

pub fn wire_hash(schema: &SchemaDef) -> u32 {
    fn mix(mut h: u32, text: &str) -> u32 {
        for byte in text.as_bytes() {
            h ^= *byte as u32;
            h = h.wrapping_mul(16_777_619);
        }
        h
    }

    let mut h = mix(2_166_136_261, &schema.name);
    h = mix(h, "|");
    h = mix(h, &schema.version.to_string());
    for field in &schema.fields {
        h = mix(h, "|");
        h = mix(h, &field.name);
        h = mix(h, ":");
        h = mix(h, &field.kind.code().to_string());
        if let Some(element) = field.element {
            h = mix(h, ".");
            h = mix(h, &element.code().to_string());
        }
        if field.flags != 0 {
            h = mix(h, ":");
            h = mix(h, &field.flags.to_string());
        }
        // An encoding is part of the wire contract: two peers that agree on the type but not the
        // bits would decode every value wrong. Mixing it in (only when it is custom) makes that a
        // hash mismatch, and keeps unannotated schemas hashing exactly as before.
        if !field.encoding_is_default() {
            h = mix(h, ":e");
            h = mix(h, &field.encoding.bits.to_string());
            h = mix(h, ",");
            h = mix(h, &field.encoding.min().to_bits().to_string());
            h = mix(h, ",");
            h = mix(h, &field.encoding.max().to_bits().to_string());
        }
        if let Some(path) = &field.nested_path {
            h = mix(h, ":n");
            h = mix(h, path);
        }
    }
    h
}
