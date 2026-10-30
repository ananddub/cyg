//! Interest management (AOI) over raw guest delta buffers.
//!
//! Near radius always visible, far radius every `skip_ticks`-th tick, beyond
//! far culled; viewer and its own bullets never culled. KD-tree keyed by
//! entity slot index (via [`crate::SpatialIndex3D`]), so negative (bot) ids
//! cannot collide.

use engine_config::AoiConfig;

#[derive(Clone, Copy, Debug)]
pub struct DeltaEntity {
    pub id: i64,
    pub pos: [f64; 3],
}

/// Guest TLV v1: `[ver u8=1][count u32]` then `count` × 72-byte records
/// `[id i64][x y z yaw pitch hp score: 7 × f64][state u32][pad u32]`, then
/// `[bullets u32][32B × bullets: x y z f64 ×3 + owner i64]`. Returns
/// `(version, entities, bullet_count, bullets_range)`; `None` on truncation.
pub fn parse_delta(delta: &[u8]) -> Option<(u32, Vec<DeltaEntity>, u32, (usize, usize))> {
    if delta.len() < 5 || delta[0] != 1 {
        return None;
    }
    let count = u32::from_le_bytes(delta[1..5].try_into().ok()?) as usize;
    if delta.len() < 5 + count * 72 + 4 {
        return None;
    }
    let mut entities = Vec::with_capacity(count);
    for i in 0..count {
        let b = 5 + i * 72;
        entities.push(DeltaEntity {
            id: i64::from_le_bytes(delta[b..b + 8].try_into().ok()?),
            pos: [
                f64::from_le_bytes(delta[b + 8..b + 16].try_into().ok()?),
                f64::from_le_bytes(delta[b + 16..b + 24].try_into().ok()?),
                f64::from_le_bytes(delta[b + 24..b + 32].try_into().ok()?),
            ],
        });
    }
    let bc_off = 5 + count * 72;
    let bullets = u32::from_le_bytes(delta[bc_off..bc_off + 4].try_into().ok()?) as usize;
    let bullet_end = bc_off + 4 + bullets * 32;
    if delta.len() < bullet_end {
        return None;
    }
    Some((
        delta[0] as u32,
        entities,
        bullets as u32,
        (bc_off, bullet_end),
    ))
}

/// AOI-filter one raw guest delta body for one viewer, writing the filtered
/// body (same TLV layout, version byte kept `1`) into `out`.
///
/// The KD-tree is keyed by entity slot index, not player id, so negative
/// (bot) ids cannot collide.
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
        // No known viewer position (e.g. before its first delta landed):
        // send everything rather than cull blindly.
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
            // Far ring rides the skip_ticks cadence; tick 0 is a boundary
            // since 0 % n == 0, so the very first delta is never short.
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

    // The viewer always sees itself, even before its first position lands.
    for (i, e) in entities.iter().enumerate() {
        if e.id == viewer_id {
            visible[i] = true;
        }
    }

    let vis_count = visible.iter().filter(|v| **v).count() as u32;
    let (bo, be) = bullets_range;
    out.clear();
    out.reserve(5 + vis_count as usize * 72 + 4 + (be - bo));
    out.push(1); // FORMAT_VERSION stays 1 — the client only knows this layout
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
        // Bullet record: `[x y z: f64×3][owner i64]` — owner is the last 8 bytes.
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

/// Convenience for the sim thread: look up a viewer's position from the
/// last-tick entity list (its own record in the delta).
pub fn viewer_pos_of(entities: &[DeltaEntity], viewer_id: i64) -> Option<[f64; 3]> {
    entities.iter().find(|e| e.id == viewer_id).map(|e| e.pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity_record(id: i64, pos: [f64; 3]) -> Vec<u8> {
        let mut r = Vec::with_capacity(72);
        r.extend_from_slice(&id.to_le_bytes());
        for v in pos {
            r.extend_from_slice(&v.to_le_bytes());
        }
        // yaw, pitch, hp, score fill the remaining f64 slots of the 72B record
        for _ in 0..4 {
            r.extend_from_slice(&0.0f64.to_le_bytes());
        }
        r.extend_from_slice(&1u32.to_le_bytes()); // state
        r.extend_from_slice(&0u32.to_le_bytes()); // pad
        assert_eq!(r.len(), 72);
        r
    }

    fn bullet_record(pos: [f64; 3], owner: i64) -> Vec<u8> {
        let mut r = Vec::with_capacity(32);
        for v in pos {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r.extend_from_slice(&owner.to_le_bytes());
        assert_eq!(r.len(), 32);
        r
    }

    /// 3 entities: viewer (id 1) at origin, near (id 2) at x=50, far (id 3)
    /// at x=300. 2 bullets: viewer-owned at x=500, foreign at x=1000.
    fn synthetic_delta() -> Vec<u8> {
        let entities: [(i64, [f64; 3]); 3] = [
            (1, [0.0, 0.0, 0.0]),
            (2, [50.0, 0.0, 0.0]),
            (3, [300.0, 0.0, 0.0]),
        ];
        let bullets: [([f64; 3], i64); 2] = [([500.0, 0.0, 0.0], 1), ([1000.0, 0.0, 0.0], 2)];
        let mut d = vec![1u8];
        d.extend_from_slice(&(entities.len() as u32).to_le_bytes());
        for (id, pos) in entities {
            d.extend_from_slice(&entity_record(id, pos));
        }
        d.extend_from_slice(&(bullets.len() as u32).to_le_bytes());
        for (pos, owner) in bullets {
            d.extend_from_slice(&bullet_record(pos, owner));
        }
        d
    }

    fn test_aoi() -> AoiConfig {
        AoiConfig {
            enabled: true,
            near_radius: 100.0,
            far_radius: 400.0,
            skip_ticks: 4,
            bullet_radius: 260.0,
        }
    }

    #[test]
    fn parse_round_trip() {
        let d = synthetic_delta();
        let (ver, entities, bc, range) = parse_delta(&d).expect("parses");
        assert_eq!(ver, 1);
        assert_eq!(entities.len(), 3);
        assert_eq!(entities[0].id, 1);
        assert_eq!(entities[1].pos, [50.0, 0.0, 0.0]);
        assert_eq!(bc, 2);
        assert_eq!(range, (5 + 3 * 72, 5 + 3 * 72 + 4 + 2 * 32));
        assert_eq!(range.1, d.len());
    }

    #[test]
    fn parse_rejects_truncation_and_bad_version() {
        let d = synthetic_delta();
        assert!(parse_delta(&d[..d.len() - 1]).is_none());
        assert!(parse_delta(&d[..10]).is_none());
        assert!(parse_delta(&[]).is_none());
        let mut bad = d.clone();
        bad[0] = 2;
        assert!(parse_delta(&bad).is_none());
    }

    #[test]
    fn near_kept_every_tick_far_only_on_skip_boundary() {
        let d = synthetic_delta();
        let (.., entities, bc, range) = parse_delta(&d).unwrap();
        // server.rs:1747 convention: the filter takes the bullet *data* range,
        // i.e. bc_off advanced past the 4-byte bullet-count field.
        let frange = (range.0 + 4, range.1);
        let aoi = test_aoi();
        let mut out = Vec::new();

        // Non-boundary tick: far entity culled; viewer self + near survive.
        filter_for_viewer(
            &entities,
            bc,
            frange,
            &d,
            1,
            Some([0.; 3]),
            3,
            &aoi,
            &mut out,
        );
        let (ver, kept, kept_bc, _) = parse_delta(&out).unwrap();
        assert_eq!(ver, 1);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|e| e.id == 1)); // viewer self
        assert!(kept.iter().any(|e| e.id == 2)); // near
        assert!(!kept.iter().any(|e| e.id == 3)); // far culled
        // Viewer-owned bullet kept despite being outside bullet_radius;
        // foreign far bullet culled.
        assert_eq!(kept_bc, 1);
        let new_bc_off = 5 + kept.len() * 72; // bullet-count field in the rewritten body
        let owner = i64::from_le_bytes(
            out[new_bc_off + 4 + 24..new_bc_off + 4 + 32]
                .try_into()
                .unwrap(),
        );
        assert_eq!(owner, 1);

        // Skip-tick boundary: far entity rides along.
        filter_for_viewer(
            &entities,
            bc,
            frange,
            &d,
            1,
            Some([0.; 3]),
            4,
            &aoi,
            &mut out,
        );
        let (_, kept, kept_bc, _) = parse_delta(&out).unwrap();
        assert_eq!(kept.len(), 3);
        assert_eq!(kept_bc, 1);
    }

    #[test]
    fn skip_ticks_one_sends_far_every_tick() {
        let d = synthetic_delta();
        let (_, entities, bc, range) = parse_delta(&d).unwrap();
        let aoi = AoiConfig {
            skip_ticks: 1,
            ..test_aoi()
        };
        let mut out = Vec::new();
        filter_for_viewer(
            &entities,
            bc,
            (range.0 + 4, range.1),
            &d,
            1,
            Some([0.; 3]),
            3,
            &aoi,
            &mut out,
        );
        let (_, kept, _, _) = parse_delta(&out).unwrap();
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn no_viewer_pos_sends_everything() {
        let d = synthetic_delta();
        let (_, entities, bc, range) = parse_delta(&d).unwrap();
        let mut out = Vec::new();
        filter_for_viewer(
            &entities,
            bc,
            (range.0 + 4, range.1),
            &d,
            1,
            None,
            3,
            &test_aoi(),
            &mut out,
        );
        let (_, kept, kept_bc, _) = parse_delta(&out).unwrap();
        assert_eq!(kept.len(), 3);
        assert_eq!(kept_bc, 2);
    }

    #[test]
    fn viewer_pos_of_finds_self_record() {
        let d = synthetic_delta();
        let (_, entities, ..) = parse_delta(&d).unwrap();
        assert_eq!(viewer_pos_of(&entities, 2), Some([50.0, 0.0, 0.0]));
        assert_eq!(viewer_pos_of(&entities, 99), None);
    }
}
