//! Host-side interest management for the schema state (`OXS2`).
//!
//! With `[aoi] enabled = true` the guest's per-tick full snapshot is treated
//! as the authoritative entity table. For each viewer the host culls rows
//! outside `near_radius` (and, on `skip_ticks` boundaries, `far_radius`) and
//! sends that viewer a *filtered full* instead of the guest patch. The client
//! reconciles full snapshots, so entities entering/leaving the view need no
//! host-side baseline and patches stay global when nothing is culled.
//!
//! Positional schemas are detected by field name: `x` + `y` (and optional
//! `z`), non-optional, numeric. A collection without those fields is passed
//! through untouched.

use std::collections::HashMap;

use engine_config::AoiConfig;
use engine_schema::{WireKind, parse_descriptor, wire_hash};

const MAGIC: &[u8; 4] = b"OXS2";
const SECTION_HEADER: usize = 13;
const WORLD_HEADER: usize = 11;

#[derive(Clone, Copy)]
struct FieldLayout {
    kind: WireKind,
    element: Option<WireKind>,
    optional: bool,
}

struct EntityLayout {
    fields: Vec<FieldLayout>,
    /// Indices of `x`, `y` and optional `z` when the schema is positional.
    position: Option<(usize, usize, Option<usize>)>,
}

/// Parsed layout for every schema a room can put on the wire. `None` from
/// [`SchemaAoi::from_descriptor`] means the project has no positional schema,
/// so the host keeps broadcasting global state.
pub struct SchemaAoi {
    by_hash: HashMap<u32, EntityLayout>,
    root_fields: Vec<FieldLayout>,
}

struct CollView<'a> {
    id: u16,
    hash: u32,
    section: &'a [u8],
    /// Parsed rows; empty when the collection has no position fields.
    rows: Vec<RowView>,
    positional: bool,
}

struct RowView {
    id: u32,
    /// Byte range of `[id][values]` inside the section.
    start: usize,
    end: usize,
    pos: [f64; 3],
}

/// One decoded full snapshot: either a bare collection (`kind 1`) or a world
/// (`kind 3`). Borrows the guest buffer and the layouts.
pub struct FullView<'a> {
    kind: u8,
    /// World header + root scalar values (`kind 3` only).
    prefix: &'a [u8],
    collections: Vec<CollView<'a>>,
}

impl SchemaAoi {
    /// Build the layout map from the same `cyg.schema.bin` bytes the
    /// host ships to clients.
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let descriptor = parse_descriptor(bytes)?;
        let mut by_hash = HashMap::new();
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

    /// Decode a full section. Returns `None` for a foreign/unknown layout, in
    /// which case the caller broadcasts the guest buffer unchanged.
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
                    let hash = u32::from_le_bytes(
                        full.get(offset + 2..offset + 6)?.try_into().ok()?,
                    );
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

    fn parse_section<'a>(&'a self, section: &'a [u8]) -> Option<CollView<'a>> {
        if section.len() < SECTION_HEADER || &section[0..4] != MAGIC || section[4] != 1 {
            return None;
        }
        let hash = u32::from_le_bytes(section[5..9].try_into().ok()?);
        let layout = self.by_hash.get(&hash)?;
        let count = u32::from_le_bytes(section[9..13].try_into().ok()?) as usize;
        let mut offset = SECTION_HEADER;
        let mut rows = Vec::new();
        for _ in 0..count {
            let start = offset;
            let id = u32::from_le_bytes(section.get(offset..offset + 4)?.try_into().ok()?);
            offset += 4;
            let mut pos = [0.0f64; 3];
            for (index, field) in layout.fields.iter().enumerate() {
                let value_at = offset;
                offset = skip_value(section, offset, field)?;
                if let Some((x, y, z)) = layout.position {
                    if index == x || index == y || Some(index) == z {
                        let value = read_number(section, value_at, field.kind)?;
                        if index == x {
                            pos[0] = value;
                        } else if index == y {
                            pos[1] = value;
                        } else {
                            pos[2] = value;
                        }
                    }
                }
            }
            if layout.position.is_some() {
                rows.push(RowView {
                    id,
                    start,
                    end: offset,
                    pos,
                });
            }
        }
        if offset != section.len() {
            return None;
        }
        Some(CollView {
            id: 0,
            hash,
            section,
            rows,
            positional: layout.position.is_some(),
        })
    }

    /// When part of the view is out of range, write this viewer's filtered
    /// full snapshot to `out` and return `true`. `false` means everything is
    /// visible (or the viewer position is unknown) and the caller should send
    /// the guest's own full/patch bytes.
    pub fn filter_culled(
        &self,
        view: &FullView<'_>,
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
            row.id == viewer_id || distance2(row.pos, viewer_pos) <= near2
                || (far_tick && distance2(row.pos, viewer_pos) <= far2)
        };

        let mut culled = false;
        for coll in &view.collections {
            if coll.positional && coll.rows.iter().any(|row| !keep(row)) {
                culled = true;
                break;
            }
        }
        if !culled {
            return false;
        }

        out.clear();
        if view.kind == 1 {
            let coll = &view.collections[0];
            write_section(coll, &keep, out);
            return true;
        }
        out.extend_from_slice(view.prefix);
        out.extend_from_slice(&(view.collections.len() as u16).to_le_bytes());
        for coll in &view.collections {
            out.extend_from_slice(&coll.id.to_le_bytes());
            out.extend_from_slice(&coll.hash.to_le_bytes());
            let len_at = out.len();
            out.extend_from_slice(&[0u8; 4]);
            let body_at = out.len();
            if coll.positional && coll.rows.iter().any(|row| !keep(row)) {
                write_section(coll, &keep, out);
            } else {
                out.extend_from_slice(coll.section);
            }
            let len = (out.len() - body_at) as u32;
            out[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
        }
        true
    }
}

fn write_section(coll: &CollView<'_>, keep: &impl Fn(&RowView) -> bool, out: &mut Vec<u8>) {
    let kept = coll.rows.iter().filter(|row| keep(row)).count();
    out.extend_from_slice(&coll.section[0..9]);
    out.extend_from_slice(&(kept as u32).to_le_bytes());
    for row in coll.rows.iter().filter(|row| keep(row)) {
        out.extend_from_slice(&coll.section[row.start..row.end]);
    }
}

fn distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn skip_value(buf: &[u8], mut offset: usize, field: &FieldLayout) -> Option<usize> {
    if field.optional {
        let present = *buf.get(offset)?;
        offset += 1;
        if present == 0 {
            return Some(offset);
        }
    }
    match field.kind {
        WireKind::String => {
            let len = *buf.get(offset)? as usize;
            offset += 1 + len;
            (offset <= buf.len()).then_some(offset)
        }
        WireKind::Array => {
            let count = *buf.get(offset)? as usize;
            offset += 1;
            let element = field.element?;
            if element.is_variable() {
                return None;
            }
            offset += count * element.size();
            (offset <= buf.len()).then_some(offset)
        }
        kind => {
            offset += kind.size();
            (offset <= buf.len()).then_some(offset)
        }
    }
}

fn read_number(buf: &[u8], offset: usize, kind: WireKind) -> Option<f64> {
    let slice = |n: usize| buf.get(offset..offset + n);
    Some(match kind {
        WireKind::U8 => *slice(1)?.first()? as f64,
        WireKind::I8 => *slice(1)?.first()? as i8 as f64,
        WireKind::Bool => (*slice(1)?.first()? != 0) as u8 as f64,
        WireKind::U16 => u16::from_le_bytes(slice(2)?.try_into().ok()?) as f64,
        WireKind::I16 => i16::from_le_bytes(slice(2)?.try_into().ok()?) as f64,
        WireKind::U32 => u32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::I32 => i32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::F32 => f32::from_le_bytes(slice(4)?.try_into().ok()?) as f64,
        WireKind::F64 => f64::from_le_bytes(slice(8)?.try_into().ok()?),
        WireKind::I64 => i64::from_le_bytes(slice(8)?.try_into().ok()?) as f64,
        WireKind::String | WireKind::Array => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_schema::{RootCollection, RootState, SchemaDef, SchemaField, emit_descriptor};

    fn field(name: &str, kind: WireKind) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            kind,
            element: None,
            ref_target: None,
            flags: 0,
            source_type: kind.typescript_type().to_string(),
            default: None,
            line: 0,
            column: 0,
        }
    }

    fn descriptor() -> Vec<u8> {
        let player = SchemaDef {
            name: "Player".into(),
            version: 1,
            fields: vec![field("x", WireKind::F64), field("y", WireKind::F64)],
            line: 0,
            column: 0,
        };
        let root = RootState {
            name: "State".into(),
            version: 1,
            scalars: Vec::new(),
            collections: vec![RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            }],
        };
        emit_descriptor(&[player], Some(&root))
    }

    /// Kind-1 full section with `(id, x, y)` rows.
    fn full_section(rows: &[(u32, f64, f64)]) -> Vec<u8> {
        let player = SchemaDef {
            name: "Player".into(),
            version: 1,
            fields: vec![field("x", WireKind::F64), field("y", WireKind::F64)],
            line: 0,
            column: 0,
        };
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.extend_from_slice(&wire_hash(&player).to_le_bytes());
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        for (id, x, y) in rows {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        }
        out
    }

    fn aoi() -> AoiConfig {
        AoiConfig {
            enabled: true,
            near_radius: 100.0,
            far_radius: 400.0,
            skip_ticks: 4,
            bullet_radius: 100.0,
        }
    }

    #[test]
    fn descriptor_round_trips_through_the_parser() {
        let bytes = descriptor();
        let parsed = parse_descriptor(&bytes).expect("descriptor parses");
        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.schemas.len(), 1);
        assert_eq!(parsed.schemas[0].fields.len(), 2);
        assert_eq!(parsed.collections, vec![("players".to_string(), 0)]);
    }

    #[test]
    fn far_entity_is_culled_on_a_non_boundary_tick() {
        let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).expect("positional schema");
        let full = full_section(&[(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 300.0, 0.0)]);
        let view = aoi_layout.view(&full).expect("full parses");
        let mut out = Vec::new();
        assert!(aoi_layout.filter_culled(&view, 1, 3, &aoi(), &mut out));
        let filtered = aoi_layout.view(&out).expect("filtered full parses");
        let ids: Vec<u32> = filtered.collections[0]
            .rows
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn far_entity_rides_a_skip_boundary() {
        let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
        let full = full_section(&[(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 300.0, 0.0)]);
        let view = aoi_layout.view(&full).unwrap();
        let mut out = Vec::new();
        assert!(!aoi_layout.filter_culled(&view, 1, 4, &aoi(), &mut out));
    }

    #[test]
    fn unknown_viewer_leaves_the_full_untouched() {
        let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
        let full = full_section(&[(1, 0.0, 0.0), (2, 900.0, 0.0)]);
        let view = aoi_layout.view(&full).unwrap();
        let mut out = Vec::new();
        assert!(!aoi_layout.filter_culled(&view, 42, 3, &aoi(), &mut out));
    }

    #[test]
    fn non_positional_schema_disables_host_views() {
        let plain = SchemaDef {
            name: "Score".into(),
            version: 1,
            fields: vec![field("points", WireKind::U32)],
            line: 0,
            column: 0,
        };
        let world = RootState {
            name: "State".into(),
            version: 1,
            scalars: Vec::new(),
            collections: vec![RootCollection {
                name: "scores".into(),
                schema: "Score".into(),
            }],
        };
        assert!(SchemaAoi::from_descriptor(&emit_descriptor(&[plain], Some(&world))).is_none());
    }
}
