use engine_schema::parse_descriptor;
use engine_schema::wire_hash;

use crate::schema_view::layout::EntityLayout;
use crate::schema_view::layout::FieldLayout;
use crate::schema_view::layout::FullView;
use crate::schema_view::layout::MAGIC;
use crate::schema_view::layout::SECTION_HEADER;
use crate::schema_view::layout::SchemaAoi;
use crate::schema_view::layout::WORLD_HEADER;
use crate::schema_view::wire::skip_value;

impl SchemaAoi {
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let descriptor = parse_descriptor(bytes)?;
        // Built once per descriptor at schema load, so exact sizing is free.
        let mut by_hash = std::collections::HashMap::with_capacity(descriptor.schemas.len());
        let mut positional = false;
        for schema in &descriptor.schemas {
            let fields: Vec<FieldLayout> = schema
                .fields
                .iter()
                .map(|field| FieldLayout {
                    kind: field.kind,
                    element: field.element,
                    optional: field.optional(),
                })
                .collect();
            let find = |name: &str| {
                schema.fields.iter().position(|field| {
                    field.name == name && !field.optional() && !field.kind.is_variable()
                })
            };
            let position = match (find("x"), find("y")) {
                (Some(x), Some(y)) => Some((x, y, find("z"))),
                _ => None,
            };
            positional |= position.is_some();
            by_hash.insert(wire_hash(schema), EntityLayout { fields, position });
        }
        if !positional {
            return None;
        }
        let root_fields = descriptor
            .root
            .scalars
            .iter()
            .map(|field| FieldLayout {
                kind: field.kind,
                element: field.element,
                optional: field.optional(),
            })
            .collect();
        Some(Self {
            by_hash,
            root_fields,
        })
    }

    pub fn view<'a>(&'a self, full: &'a [u8]) -> Option<FullView<'a>> {
        if full.len() < SECTION_HEADER || &full[0..4] != MAGIC {
            return None;
        }
        match full[4] {
            1 => {
                let coll = self.parse_section(full)?;
                Some(FullView {
                    kind: 1,
                    prefix: &[],
                    collections: vec![coll],
                })
            }
            3 => {
                if full.len() < WORLD_HEADER {
                    return None;
                }
                let root_count = u16::from_le_bytes(full[9..11].try_into().ok()?) as usize;
                if root_count != self.root_fields.len() {
                    return None;
                }
                let mut offset = WORLD_HEADER;
                for field in &self.root_fields {
                    offset = skip_value(full, offset, field)?;
                }
                let prefix = &full[..offset];
                let count =
                    u16::from_le_bytes(full.get(offset..offset + 2)?.try_into().ok()?) as usize;
                offset += 2;
                let mut collections = Vec::with_capacity(count);
                for _ in 0..count {
                    let id = u16::from_le_bytes(full.get(offset..offset + 2)?.try_into().ok()?);
                    let hash =
                        u32::from_le_bytes(full.get(offset + 2..offset + 6)?.try_into().ok()?);
                    let len =
                        u32::from_le_bytes(full.get(offset + 6..offset + 10)?.try_into().ok()?)
                            as usize;
                    offset += 10;
                    let section = full.get(offset..offset + len)?;
                    offset += len;
                    let mut coll = self.parse_section(section)?;
                    if coll.hash != hash {
                        return None;
                    }
                    coll.id = id;
                    collections.push(coll);
                }
                if offset != full.len() {
                    return None;
                }
                Some(FullView {
                    kind: 3,
                    prefix,
                    collections,
                })
            }
            _ => None,
        }
    }
}
