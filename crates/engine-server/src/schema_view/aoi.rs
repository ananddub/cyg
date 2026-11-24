use engine_config::AoiConfig;
use engine_webrtc_protocol::packed::{self, PackedOp, WireValue};

use crate::schema_view::layout::{CollView, EntityLayout, FullView, RowView, SchemaAoi};

impl SchemaAoi {
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let descriptor = engine_schema::parse_descriptor(bytes)?;
        let mut by_hash = std::collections::HashMap::with_capacity(descriptor.schemas.len());
        let mut positional = false;
        for schema in &descriptor.schemas {
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
            let hash = engine_schema::wire_hash(schema);
            by_hash.insert(
                hash,
                EntityLayout {
                    packed: packed::PackedSchema::from_def(schema),
                    position,
                },
            );
        }
        if !positional {
            return None;
        }
        let root_schema = engine_schema::SchemaDef {
            nested_arrays: Vec::new(),
            name: descriptor.root.name.clone(),
            version: descriptor.root.version,
            fields: descriptor.root.scalars.clone(),
            line: 0,
            column: 0,
        };
        let root_packed = packed::PackedSchema::from_def(&root_schema);
        Some(Self {
            by_hash,
            root_packed,
        })
    }

    pub fn view(&self, full: &[u8]) -> Option<FullView> {
        if full.len() < 5 || &full[0..4] != b"OXS2" {
            return None;
        }
        let kind = full[4];
        if kind == packed::KIND_FULL {
            let hash = u32::from_le_bytes(full.get(5..9)?.try_into().ok()?);
            let layout = self.by_hash.get(&hash)?;
            let decoded = packed::decode(&layout.packed, full).ok()?;
            let mut rows = Vec::with_capacity(decoded.ops.len());
            for op in decoded.ops {
                let PackedOp::Add { id, values } = op else {
                    return None;
                };
                rows.push(make_row(id, values, layout.position));
            }
            return Some(FullView {
                kind,
                root: Vec::new(),
                collections: vec![CollView {
                    id: 0,
                    hash,
                    rows,
                    positional: layout.position.is_some(),
                }],
            });
        }
        if kind != packed::KIND_WORLD_FULL {
            return None;
        }
        let world = packed::decode_world(&self.root_packed, full).ok()?;
        let mut collections = Vec::with_capacity(world.collections.len());
        for (id, hash, bytes) in world.collections {
            let layout = self.by_hash.get(&hash)?;
            let decoded = packed::decode(&layout.packed, &bytes).ok()?;
            if decoded.kind != packed::KIND_FULL {
                return None;
            }
            let mut rows = Vec::with_capacity(decoded.ops.len());
            for op in decoded.ops {
                let PackedOp::Add { id, values } = op else {
                    return None;
                };
                rows.push(make_row(id, values, layout.position));
            }
            collections.push(CollView {
                id,
                hash,
                rows,
                positional: layout.position.is_some(),
            });
        }
        Some(FullView {
            kind,
            root: world.root,
            collections,
        })
    }

    pub fn filter_culled(
        &self,
        view: &FullView,
        viewer_id: u32,
        tick: u64,
        aoi: &AoiConfig,
        out: &mut Vec<u8>,
    ) -> bool {
        let Some(viewer_pos) = view
            .collections
            .iter()
            .flat_map(|coll| &coll.rows)
            .find(|row| row.id == viewer_id)
            .map(|row| row.pos)
        else {
            return false;
        };
        let near2 = aoi.near_radius * aoi.near_radius;
        let far = aoi.far_radius.max(aoi.near_radius);
        let far2 = far * far;
        let far_tick = aoi.skip_ticks <= 1 || tick % aoi.skip_ticks == 0;
        let keep = |row: &RowView| {
            row.id == viewer_id
                || distance2(row.pos, viewer_pos) <= near2
                || (far_tick && distance2(row.pos, viewer_pos) <= far2)
        };
        if !view
            .collections
            .iter()
            .any(|coll| coll.positional && coll.rows.iter().any(|row| !keep(row)))
        {
            return false;
        }
        let mut clamped = 0;
        if view.kind == packed::KIND_FULL {
            let coll = &view.collections[0];
            let layout = &self.by_hash[&coll.hash];
            let rows: Vec<_> = coll
                .rows
                .iter()
                .filter(|row| keep(row))
                .map(|row| (row.id, row.values.clone()))
                .collect();
            if let Ok(bytes) = packed::encode_full(&layout.packed, &rows, &mut clamped) {
                *out = bytes;
                return true;
            }
            return false;
        }
        let collections: Vec<_> = view
            .collections
            .iter()
            .map(|coll| {
                let layout = &self.by_hash[&coll.hash];
                let rows: Vec<_> = coll
                    .rows
                    .iter()
                    .filter(|row| !coll.positional || keep(row))
                    .map(|row| (row.id, row.values.clone()))
                    .collect();
                let bytes =
                    packed::encode_full(&layout.packed, &rows, &mut clamped).unwrap_or_default();
                (coll.id, coll.hash, bytes)
            })
            .collect();
        match packed::encode_world_full(
            &self.root_packed,
            &view.root.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>(),
            &collections,
            &mut clamped,
        ) {
            Ok(bytes) => {
                *out = bytes;
                true
            }
            Err(_) => false,
        }
    }
}

fn make_row(
    id: u32,
    values: Vec<WireValue>,
    position: Option<(usize, usize, Option<usize>)>,
) -> RowView {
    let mut pos = [0.0; 3];
    if let Some((x, y, z)) = position {
        pos[0] = values.get(x).map(number).unwrap_or(0.0);
        pos[1] = values.get(y).map(number).unwrap_or(0.0);
        if let Some(z) = z {
            pos[2] = values.get(z).map(number).unwrap_or(0.0);
        }
    }
    RowView { id, pos, values }
}

fn number(value: &WireValue) -> f64 {
    match value {
        WireValue::U8(v) => *v as f64,
        WireValue::U16(v) => *v as f64,
        WireValue::U32(v) => *v as f64,
        WireValue::I8(v) => *v as f64,
        WireValue::I16(v) => *v as f64,
        WireValue::I32(v) => *v as f64,
        WireValue::I64(v) => *v as f64,
        WireValue::F32(v) => *v as f64,
        WireValue::F64(v) => *v,
        WireValue::Bool(v) => u8::from(*v) as f64,
        _ => 0.0,
    }
}

fn distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}
