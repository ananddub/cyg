use engine_config::AoiConfig;

use super::filter_for_viewer_indexed;
use super::parse_delta;
use super::support::{bullet_record, entity_record, test_aoi};
use super::view_index::AoiViewIndex;
use super::DeltaEntity;

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn rand_range(state: &mut u64, lo: i64, hi: i64) -> i64 {
    lo + (xorshift(state) % (hi - lo + 1) as u64) as i64
}

/// Brute-force linear reference: computes the visible mask by scanning all
/// entities per viewer, exactly like the original (pre-index) filter.
fn brute_force_visible(
    entities: &[DeltaEntity],
    viewer_id: i64,
    viewer_pos: Option<[f64; 3]>,
    tick: u64,
    aoi: &AoiConfig,
) -> Vec<bool> {
    let mut visible = vec![false; entities.len()];
    if let Some(vp) = viewer_pos {
        for (i, e) in entities.iter().enumerate() {
            let (dx, dy, dz) = (e.pos[0] - vp[0], e.pos[1] - vp[1], e.pos[2] - vp[2]);
            let d2 = dx * dx + dy * dy + dz * dz;
            if d2 <= aoi.near_radius * aoi.near_radius {
                visible[i] = true;
            }
            if aoi.skip_ticks <= 1 || tick % aoi.skip_ticks == 0 {
                let far = aoi.far_radius.max(aoi.near_radius);
                if d2 <= far * far {
                    visible[i] = true;
                }
            }
        }
    } else {
        visible.iter_mut().for_each(|v| *v = true);
    }
    for (i, e) in entities.iter().enumerate() {
        if e.id == viewer_id {
            visible[i] = true;
        }
    }
    visible
}

fn build_delta(entities: &[DeltaEntity], bullets: &[([f64; 3], i64)]) -> Vec<u8> {
    let mut d = vec![1u8];
    d.extend_from_slice(&(entities.len() as u32).to_le_bytes());
    for e in entities {
        d.extend_from_slice(&entity_record(e.id, e.pos));
    }
    d.extend_from_slice(&(bullets.len() as u32).to_le_bytes());
    for (pos, owner) in bullets {
        d.extend_from_slice(&bullet_record(*pos, *owner));
    }
    d
}

/// Compares the indexed filter against a brute-force linear reference over
/// randomized worlds: identical visible set (whole output buffer) per viewer.
#[test]
fn indexed_filter_matches_brute_force() {
    let mut state = 0xD1CE_C0DE_u64;
    for world in 0..200 {
        let n = rand_range(&mut state, 0, 300) as usize;
        let entities: Vec<DeltaEntity> = (0..n)
            .map(|i| DeltaEntity {
                id: if i == 0 {
                    7 // deterministic viewer id candidate
                } else {
                    rand_range(&mut state, -50, 50)
                },
                pos: [
                    rand_range(&mut state, -1500, 1500) as f64,
                    rand_range(&mut state, -1500, 1500) as f64,
                    rand_range(&mut state, -1500, 1500) as f64,
                ],
            })
            .collect();

        let bullet_count = rand_range(&mut state, 0, 20) as usize;
        let bullets: Vec<([f64; 3], i64)> = (0..bullet_count)
            .map(|_| {
                (
                    [
                        rand_range(&mut state, -1500, 1500) as f64,
                        rand_range(&mut state, -1500, 1500) as f64,
                        rand_range(&mut state, -1500, 1500) as f64,
                    ],
                    rand_range(&mut state, -50, 50),
                )
            })
            .collect();

        let delta = build_delta(&entities, &bullets);
        let (_, parsed_entities, bc, range) = parse_delta(&delta).unwrap();
        assert_eq!(parsed_entities, entities);
        let frange = (range.0 + 4, range.1);

        let aoi = AoiConfig {
            near_radius: rand_range(&mut state, 20, 300) as f64,
            far_radius: rand_range(&mut state, 100, 700) as f64,
            skip_ticks: rand_range(&mut state, 1, 5) as u64,
            bullet_radius: rand_range(&mut state, 50, 400) as f64,
            ..test_aoi()
        };
        let tick = rand_range(&mut state, 0, 20) as u64;

        // One index per tick, queried by every viewer.
        let mut index = AoiViewIndex::new(aoi.far_radius);
        index.rebuild(&entities);

        let viewers = rand_range(&mut state, 1, 8) as usize;
        for v in 0..viewers {
            let viewer_id = if v % 2 == 0 {
                7
            } else {
                rand_range(&mut state, -50, 50)
            };
            // Viewer position: either an entity's position or a random spot.
            let viewer_pos = if rand_range(&mut state, 0, 3) == 0 {
                None
            } else if !entities.is_empty() && rand_range(&mut state, 0, 1) == 0 {
                Some(entities[rand_range(&mut state, 0, entities.len() as i64 - 1) as usize].pos)
            } else {
                Some([
                    rand_range(&mut state, -1500, 1500) as f64,
                    rand_range(&mut state, -1500, 1500) as f64,
                    rand_range(&mut state, -1500, 1500) as f64,
                ])
            };

            let mut out_indexed = Vec::new();
            let mut visible_scratch = Vec::with_capacity(entities.len());
            filter_for_viewer_indexed(
                &entities,
                bc,
                frange,
                &delta,
                viewer_id,
                viewer_pos,
                tick,
                &aoi,
                &mut index,
                &mut out_indexed,
                &mut visible_scratch,
            );

            // Reference: same pipeline but with a linear brute-force mask.
            let visible = brute_force_visible(&entities, viewer_id, viewer_pos, tick, &aoi);
            let mut out_ref = Vec::new();
            super::filter::emit_filtered(
                &entities, bc, frange, &delta, viewer_id, &visible, &aoi, viewer_pos, &mut out_ref,
            );

            assert_eq!(
                out_indexed, out_ref,
                "world={world} viewer={v} viewer_id={viewer_id} viewer_pos={viewer_pos:?}"
            );
        }
    }
}
