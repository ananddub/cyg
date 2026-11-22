use crate::model::{RootState, SchemaDef, SchemaField};
use crate::wire::WireKind;
use crate::{DESCRIPTOR_MAGIC, DESCRIPTOR_VERSION, wire_hash};

fn write_u16(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u16).to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_str(out: &mut Vec<u8>, value: &str) {
    write_u16(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

fn write_field(out: &mut Vec<u8>, field: &SchemaField) {
    write_str(out, &field.name);
    out.push(field.kind.code());
    out.push(field.flags);
    out.push(field.element.map(WireKind::code).unwrap_or(0));
    write_str(out, field.nested_path.as_deref().unwrap_or(""));
    // Bit 7 carries the integer flag; the width is at most 32, so it always fits below it.
    out.push(field.encoding.bits | if field.encoding.integer { 0x80 } else { 0 });
    out.extend_from_slice(&field.encoding.min().to_bits().to_le_bytes());
    out.extend_from_slice(&field.encoding.max().to_bits().to_le_bytes());
}

/// The wire contract has exactly one live version. Older descriptors are
/// rejected by the reader with a "rebuild client/server" error; there is no
/// dual parser and no silent fallback.
pub fn emit_descriptor(schemas: &[SchemaDef], root: Option<&RootState>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(DESCRIPTOR_MAGIC);
    out.push(DESCRIPTOR_VERSION);
    let nested_meta = root.is_some_and(|root| !root.nested.is_empty());
    out.push(if nested_meta { 1 } else { 0 });
    write_u16(&mut out, schemas.len());
    for schema in schemas {
        write_str(&mut out, &schema.name);
        write_u32(&mut out, schema.version);
        write_u32(&mut out, wire_hash(schema));
        write_u16(&mut out, schema.fields.len());
        for field in &schema.fields {
            write_field(&mut out, field);
        }
    }

    let empty = RootState {
        nested: Vec::new(),
        name: String::new(),
        version: 0,
        scalars: Vec::new(),
        collections: Vec::new(),
    };
    let root = root.unwrap_or(&empty);
    let root_schema = SchemaDef {
        nested_arrays: Vec::new(),
        name: root.name.clone(),
        version: root.version,
        fields: root.scalars.clone(),
        line: 0,
        column: 0,
    };
    write_str(&mut out, &root.name);
    write_u32(&mut out, root.version);
    write_u32(&mut out, wire_hash(&root_schema));
    write_u16(&mut out, root.scalars.len());
    for field in &root.scalars {
        write_field(&mut out, field);
    }

    write_u16(&mut out, root.collections.len());
    for collection in &root.collections {
        write_str(&mut out, &collection.name);
        let index = schemas
            .iter()
            .position(|schema| schema.name == collection.schema)
            .unwrap_or(u16::MAX as usize);
        write_u16(&mut out, index);
    }
    if nested_meta {
        write_u16(&mut out, root.nested.len());
        for nested in &root.nested {
            write_str(&mut out, &nested.collection);
            write_str(&mut out, &nested.parent);
            write_str(&mut out, &nested.field);
        }
    }
    out
}
