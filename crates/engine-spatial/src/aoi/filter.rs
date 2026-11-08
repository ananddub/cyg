use engine_config::AoiConfig;

use super::DeltaEntity;

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
    let mut visible: Vec<bool> = vec![false; entities.len()];
    match viewer_pos {
        None => visible.iter_mut().for_each(|v| *v = true),
        Some(vp) => {
            let mut index = crate::SpatialIndex3D::new();
            for (i, e) in entities.iter().enumerate() {
                index.insert(i as u32, e.pos);
            }
            for r in index.query_radius(vp, aoi.near_radius) {
                let idx = r.entity_id as usize;
                if idx < entities.len() {
                    visible[idx] = true;
                }
            }

            if aoi.skip_ticks <= 1 || tick % aoi.skip_ticks == 0 {
                for r in index.query_radius(vp, aoi.far_radius.max(aoi.near_radius)) {
                    let idx = r.entity_id as usize;
                    if idx < entities.len() {
                        visible[idx] = true;
                    }
                }
            }
        }
    }

    for (i, e) in entities.iter().enumerate() {
        if e.id == viewer_id {
            visible[i] = true;
        }
    }

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
