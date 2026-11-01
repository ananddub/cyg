use engine_config::AoiConfig;

use crate::schema_view::layout::CollView;
use crate::schema_view::layout::FullView;
use crate::schema_view::layout::MAGIC;
use crate::schema_view::layout::RowView;
use crate::schema_view::layout::SECTION_HEADER;
use crate::schema_view::layout::SchemaAoi;
use crate::schema_view::wire::read_number;
use crate::schema_view::wire::skip_value;

impl SchemaAoi {
    pub(super) fn parse_section<'a>(&'a self, section: &'a [u8]) -> Option<CollView<'a>> {
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
            row.id == viewer_id
                || distance2(row.pos, viewer_pos) <= near2
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
