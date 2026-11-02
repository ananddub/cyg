use std::fmt;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootCollection {
    pub name: String,
    pub schema: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootState {
    pub name: String,
    pub version: u32,
    pub scalars: Vec<SchemaField>,
    pub collections: Vec<RootCollection>,
}

pub const MAX_COLLECTIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaDef {
    pub name: String,
    pub version: u32,
    pub fields: Vec<SchemaField>,
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
    }
    h
}
