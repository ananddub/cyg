use engine_config::AoiConfig;

use super::DeltaEntity;
use super::view_index::AoiViewIndex;

/// Per-viewer AOI filter using a shared [`AoiViewIndex`] built once per tick
/// (see [`AoiViewIndex::rebuild`]). Produces byte-identical output to the old
/// per-viewer kd-tree implementation.
///
/// `visible` is a caller-owned scratch buffer, cleared and resized here —
/// a fresh `vec![false; n]` per viewer per tick was the dominant allocator
/// cost at high viewer counts (one allocation per viewer per tick).
pub fn filter_for_viewer_indexed(
    entities: &[DeltaEntity],
    bullet_count: u32,
    bullets_range: (usize, usize),
    delta: &[u8],
    viewer_id: i64,
    viewer_pos: Option<[f64; 3]>,
    tick: u64,
    aoi: &AoiConfig,
    index: &mut AoiViewIndex,
    out: &mut Vec<u8>,
    visible: &mut Vec<bool>,
) {
    visible.clear();
    visible.resize(entities.len(), false);
    match viewer_pos {
        None => visible.iter_mut().for_each(|v| *v = true),
        Some(vp) => {
            index.query_squared(vp, aoi.near_radius * aoi.near_radius, |idx| {
                if idx < entities.len() {
                    visible[idx] = true;
                }
            });

            if aoi.skip_ticks <= 1 || tick % aoi.skip_ticks == 0 {
                let far = aoi.far_radius.max(aoi.near_radius);
                index.query_squared(vp, far * far, |idx| {
                    if idx < entities.len() {
                        visible[idx] = true;
                    }
                });
            }
        }
    }

    for (i, e) in entities.iter().enumerate() {
        if e.id == viewer_id {
            visible[i] = true;
        }
    }

    emit_filtered(
        entities, bullet_count, bullets_range, delta, viewer_id, &visible, aoi, viewer_pos, out,
    );
}

/// Legacy entry point: builds a throwaway index per call. Prefer building one
/// [`AoiViewIndex`] per tick and calling [`filter_for_viewer_indexed`] per
/// viewer.
pub fn filter_for_viewer(
    entities: &[DeltaEntity],
    bullet_count: u32,
    bullets_range: (usize, usize),
    delta: &[u8],
    viewer_id: i64,
    viewer_pos: Option<[f64; 3]>,
    tick: u64,
    aoi: &AoiConfig,
    out: &mut Vec<u8>,
) {
    let mut index = AoiViewIndex::new(aoi.far_radius);
    index.rebuild(entities);
    let mut visible = Vec::with_capacity(entities.len());
    filter_for_viewer_indexed(
        entities,
        bullet_count,
        bullets_range,
        delta,
        viewer_id,
        viewer_pos,
        tick,
        aoi,
        &mut index,
        out,
        &mut visible,
    );
}

pub(crate) fn emit_filtered(
    entities: &[DeltaEntity],
    bullet_count: u32,
    bullets_range: (usize, usize),
    delta: &[u8],
    viewer_id: i64,
    visible: &[bool],
    aoi: &AoiConfig,
    viewer_pos: Option<[f64; 3]>,
    out: &mut Vec<u8>,
) {
    let vis_count = visible.iter().filter(|v| **v).count() as u32;
    let (bo, be) = bullets_range;
    out.clear();
    out.reserve(5 + vis_count as usize * 72 + 4 + (be - bo));
    out.push(1);
    out.extend_from_slice(&vis_count.to_le_bytes());
    for i in 0..entities.len() {
        if visible[i] {
            let b = 5 + i * 72;
            out.extend_from_slice(&delta[b..b + 72]);
        }
    }

    let mut vis_bullets: Vec<usize> = Vec::new();
    for idx in 0..bullet_count as usize {
        // (bullets are few — collecting indices before the second pass keeps
        // the count prefix correct; the Vec alloc per call is one small alloc)
        let b = bo + idx * 32;
        if b + 32 > be {
            break;
        }

        let owner = i64::from_le_bytes(delta[b + 24..b + 32].try_into().unwrap_or([0; 8]));
        let keep = owner == viewer_id
            || viewer_pos.map_or(true, |vp| {
                let bx = f64::from_le_bytes(delta[b..b + 8].try_into().unwrap_or([0; 8]));
                let by = f64::from_le_bytes(delta[b + 8..b + 16].try_into().unwrap_or([0; 8]));
                let bz = f64::from_le_bytes(delta[b + 16..b + 24].try_into().unwrap_or([0; 8]));
                (bx - vp[0]) * (bx - vp[0])
                    + (by - vp[1]) * (by - vp[1])
                    + (bz - vp[2]) * (bz - vp[2])
                    <= aoi.bullet_radius * aoi.bullet_radius
            });
        if keep {
            vis_bullets.push(b);
        }
    }
    out.extend_from_slice(&(vis_bullets.len() as u32).to_le_bytes());
    for b in vis_bullets {
        out.extend_from_slice(&delta[b..b + 32]);
    }
}
