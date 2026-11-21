use super::{DESCRIPTOR_MAGIC, DESCRIPTOR_VERSION, Reader};
use crate::model::{RootCollection, RootState, SchemaDef, SchemaField};
use crate::encoding::{FieldEncoding, default_encoding};
use crate::wire::WireKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub version: u8,
    pub schemas: Vec<SchemaDef>,
    pub root: RootState,

    pub collections: Vec<(String, usize)>,
}

fn read_descriptor_field(r: &mut Reader<'_>, version: u8) -> Option<SchemaField> {
    let name = r.str()?;
    let kind = WireKind::from_code(r.u8()?)?;
    let mut flags = 0;
    let mut element = None;
    if version >= 3 {
        flags = r.u8()?;
        let element_code = r.u8()?;
        if element_code != 0 {
            element = Some(WireKind::from_code(element_code)?);
        }
    }
    let mut encoding = default_encoding(kind);
    if version >= 4 {
        let packed = r.u8()?;
        let min = f32::from_bits(r.u32()?);
        let max = f32::from_bits(r.u32()?);
        encoding = FieldEncoding::range(packed & 0x7F, min, max);
        if packed & 0x80 != 0 {
            encoding = encoding.as_integer();
        }
    }
    Some(SchemaField {
        name,
        kind,
        element,
        ref_target: None,
        flags,
        source_type: kind.typescript_type().to_string(),
        default: None,
        encoding,
        line: 0,
        column: 0,
    })
}

pub fn parse_descriptor(bytes: &[u8]) -> Option<Descriptor> {
    if bytes.len() < 6 || &bytes[0..4] != DESCRIPTOR_MAGIC {
        return None;
    }
    let mut r = Reader { bytes, offset: 4 };
    let version = r.u8()?;
    if version < 1 || version > DESCRIPTOR_VERSION {
        return None;
    }
    let _flags = r.u8()?;
    let schema_count = r.u16()? as usize;
    let mut schemas = Vec::with_capacity(schema_count);
    for _ in 0..schema_count {
        let name = r.str()?;
        let schema_version = r.u32()?;
        let _hash = r.u32()?;
        let field_count = r.u16()? as usize;
        let mut fields = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            fields.push(read_descriptor_field(&mut r, version)?);
        }
        schemas.push(SchemaDef {
            name,
            version: schema_version,
            fields,
            line: 0,
            column: 0,
        });
    }

    let root_name = r.str()?;
    let mut collections: Vec<(String, usize)> = Vec::new();
    let root = if version == 1 {
        let index = r.u16()? as usize;
        schemas.get(index)?;
        if !root_name.is_empty() {
            collections.push((root_name.clone(), index));
        }
        RootState {
            name: root_name,
            version: 0,
            scalars: Vec::new(),
            collections: collections
                .iter()
                .map(|(name, index)| RootCollection {
                    name: name.clone(),
                    schema: schemas[*index].name.clone(),
                })
                .collect(),
        }
    } else {
        let root_version = r.u32()?;
        let _root_hash = r.u32()?;
        let scalar_count = r.u16()? as usize;
        let mut scalars = Vec::with_capacity(scalar_count);
        for _ in 0..scalar_count {
            scalars.push(read_descriptor_field(&mut r, version)?);
        }
        let collection_count = r.u16()? as usize;
        for _ in 0..collection_count {
            let name = r.str()?;
            let index = r.u16()? as usize;
            schemas.get(index)?;
            collections.push((name, index));
        }
        RootState {
            name: root_name,
            version: root_version,
            scalars,
            collections: collections
                .iter()
                .map(|(name, index)| RootCollection {
                    name: name.clone(),
                    schema: schemas[*index].name.clone(),
                })
                .collect(),
        }
    };
    if r.offset != bytes.len() {
        return None;
    }
    Some(Descriptor {
        version,
        schemas,
        root,
        collections,
    })
}
