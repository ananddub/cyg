use super::DeltaEntity;

const MIN_CELL_SIZE: f64 = 64.0;
const BUCKET_BITS: u32 = 12;
const BUCKET_COUNT: usize = 1 << BUCKET_BITS;
const BUCKET_MASK: u64 = (BUCKET_COUNT - 1) as u64;

/// Shared per-tick AOI index over a delta's entity positions.
///
/// Built ONCE per tick (before the per-viewer loop) from the parsed
/// [`DeltaEntity`] slice, then queried once per viewer with zero allocations.
/// This replaces the previous design where `filter_for_viewer` built a fresh
/// kd-tree (N inserts + several Vec allocations) for every viewer on every
/// tick.
///
/// The structure is a uniform spatial-hash grid following the
/// `engine-physics::collision::grid::StaticObstacleGrid` pattern: entities are
/// bucketed by cell coordinates, queries walk the cells overlapping the query
/// sphere and apply an exact squared-distance check per candidate, so results
/// are identical to a linear (or kd-tree) radius query.
pub struct AoiViewIndex {
    cell_size: f64,
    /// Entity positions by slice index (copied at rebuild).
    positions: Vec<[f64; 3]>,
    /// Entity ids by slice index (copied at rebuild).
    ids: Vec<i64>,
    /// Spatial-hash buckets holding entity slice indices.
    cells: Vec<Vec<u32>>,
    /// Per-entity visit stamps for duplicate-free queries (epoch trick).
    stamps: Vec<u32>,
    epoch: u32,
    /// Inclusive cell-coordinate bounds of the entities inserted at rebuild,
    /// used to clamp query cell walks.
    cell_bounds: [i64; 6],
}

impl Default for AoiViewIndex {
    fn default() -> Self {
        Self::new(MIN_CELL_SIZE)
    }
}

impl AoiViewIndex {
    /// `radius_hint` should be the largest AOI query radius (typically
    /// `AoiConfig::far_radius`); it sets the cell size so each query touches
    /// only a small neighborhood of cells.
    pub fn new(radius_hint: f64) -> Self {
        Self {
            cell_size: radius_hint.max(MIN_CELL_SIZE),
            positions: Vec::new(),
            ids: Vec::new(),
            cells: vec![Vec::new(); BUCKET_COUNT],
            stamps: Vec::new(),
            epoch: 0,
            cell_bounds: [0; 6],
        }
    }

    /// Rebuild the grid for a new tick's entity set. Reuses all allocations
    /// (buckets, position/id/stamp buffers), so steady-state rebuilds are
    /// allocation-free.
    pub fn rebuild(&mut self, entities: &[DeltaEntity]) {
        self.positions.clear();
        self.ids.clear();
        self.positions.extend(entities.iter().map(|e| e.pos));
        self.ids.extend(entities.iter().map(|e| e.id));

        if self.stamps.len() != entities.len() {
            self.stamps.clear();
            self.stamps.resize(entities.len(), 0);
        } else {
            self.stamps.iter_mut().for_each(|s| *s = 0);
        }
        self.epoch = 0;

        for cell in &mut self.cells {
            cell.clear();
        }

        let cell_size = self.cell_size;
        self.cell_bounds = [i64::MAX, i64::MIN, i64::MAX, i64::MIN, i64::MAX, i64::MIN];
        for (i, e) in entities.iter().enumerate() {
            let (cx, cy, cz) = (
                cell_coord(e.pos[0], cell_size),
                cell_coord(e.pos[1], cell_size),
                cell_coord(e.pos[2], cell_size),
            );
            self.cell_bounds[0] = self.cell_bounds[0].min(cx);
            self.cell_bounds[1] = self.cell_bounds[1].max(cx);
            self.cell_bounds[2] = self.cell_bounds[2].min(cy);
            self.cell_bounds[3] = self.cell_bounds[3].max(cy);
            self.cell_bounds[4] = self.cell_bounds[4].min(cz);
            self.cell_bounds[5] = self.cell_bounds[5].max(cz);
            let bucket = self.bucket_of_cell(cx, cy, cz);
            self.cells[bucket].push(i as u32);
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn position(&self, index: usize) -> Option<[f64; 3]> {
        self.positions.get(index).copied()
    }

    pub fn id(&self, index: usize) -> Option<i64> {
        self.ids.get(index).copied()
    }

    /// Zero-alloc radius query. The visitor receives each entity slice index
    /// whose squared distance to `center` is `<= radius_sq`, at most once.
    /// Exact check per candidate — identical results to a linear scan.
    pub fn query_squared(&mut self, center: [f64; 3], radius_sq: f64, mut f: impl FnMut(usize)) {
        if self.positions.is_empty() {
            return;
        }

        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamps.iter_mut().for_each(|s| *s = 0);
            self.epoch = 1;
        }
        let epoch = self.epoch;

        let radius = radius_sq.max(0.0).sqrt();
        let cell_size = self.cell_size;
        let (cx, cy, cz) = (
            cell_coord(center[0], cell_size),
            cell_coord(center[1], cell_size),
            cell_coord(center[2], cell_size),
        );
        let span = ((radius / cell_size).max(0.0) as i64).saturating_add(1);
        // Saturating cell-range arithmetic: `span` may exceed i64 bounds for
        // unreasonably large radii; the per-axis clamps below bound the walk.
        let lo = |c: i64, s: i64, min: i64| c.saturating_sub(s).max(min);
        let hi = |c: i64, s: i64, max: i64| c.saturating_add(s).min(max);
        let (min_cx, max_cx) = (self.cell_bounds[0], self.cell_bounds[1]);
        let (min_cy, max_cy) = (self.cell_bounds[2], self.cell_bounds[3]);
        let (min_cz, max_cz) = (self.cell_bounds[4], self.cell_bounds[5]);

        for cz in lo(cz, span, min_cz)..=hi(cz, span, max_cz) {
            for cy in lo(cy, span, min_cy)..=hi(cy, span, max_cy) {
                for cx in lo(cx, span, min_cx)..=hi(cx, span, max_cx) {
                    let bucket = self.bucket_of_cell(cx, cy, cz);
                    for &idx in &self.cells[bucket] {
                        let i = idx as usize;
                        if self.stamps[i] == epoch {
                            continue;
                        }
                        self.stamps[i] = epoch;
                        let p = self.positions[i];
                        let ddx = p[0] - center[0];
                        let ddy = p[1] - center[1];
                        let ddz = p[2] - center[2];
                        if ddx * ddx + ddy * ddy + ddz * ddz <= radius_sq {
                            f(i);
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn bucket_of_pos(&self, pos: [f64; 3], cell_size: f64) -> usize {
        self.bucket_of_cell(
            cell_coord(pos[0], cell_size),
            cell_coord(pos[1], cell_size),
            cell_coord(pos[2], cell_size),
        )
    }

    #[inline]
    fn bucket_of_cell(&self, x: i64, y: i64, z: i64) -> usize {
        let h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        (h & BUCKET_MASK) as usize
    }
}

#[inline]
fn cell_coord(v: f64, cell_size: f64) -> i64 {
    let c = (v / cell_size).floor();
    // Saturating cast; NaN maps to 0. Entities at non-finite coordinates can
    // never satisfy an exact squared-distance check anyway.
    if c.is_finite() { c as i64 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn query_matches_linear_scan() {
        let mut state = 0x5EED_C0FF_EE01_u64;
        for _ in 0..200 {
            let n = rand_range(&mut state, 0, 300) as usize;
            let entities: Vec<DeltaEntity> = (0..n)
                .map(|_| DeltaEntity {
                    id: rand_range(&mut state, -1000, 1000),
                    pos: [
                        rand_range(&mut state, -2000, 2000) as f64,
                        rand_range(&mut state, -2000, 2000) as f64,
                        rand_range(&mut state, -2000, 2000) as f64,
                    ],
                })
                .collect();

            let mut index = AoiViewIndex::new(420.0);
            index.rebuild(&entities);

            for _ in 0..8 {
                let center = [
                    rand_range(&mut state, -2500, 2500) as f64,
                    rand_range(&mut state, -2500, 2500) as f64,
                    rand_range(&mut state, -2500, 2500) as f64,
                ];
                let radius = rand_range(&mut state, 0, 900) as f64;
                let radius_sq = radius * radius;

                let mut got: Vec<usize> = Vec::new();
                index.query_squared(center, radius_sq, |i| got.push(i));
                got.sort_unstable();

                let want: Vec<usize> = entities
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        let (dx, dy, dz) = (
                            e.pos[0] - center[0],
                            e.pos[1] - center[1],
                            e.pos[2] - center[2],
                        );
                        dx * dx + dy * dy + dz * dz <= radius_sq
                    })
                    .map(|(i, _)| i)
                    .collect();

                assert_eq!(
                    got, want,
                    "entities={entities:?} center={center:?} r={radius}"
                );
            }
        }
    }

    #[test]
    fn rebuild_reuses_buckets_across_ticks() {
        let mut state = 0xBEEF_u64;
        let mut index = AoiViewIndex::new(420.0);
        for _ in 0..50 {
            let n = rand_range(&mut state, 1, 200) as usize;
            let entities: Vec<DeltaEntity> = (0..n)
                .map(|_| DeltaEntity {
                    id: rand_range(&mut state, -100, 100),
                    pos: [
                        rand_range(&mut state, -1000, 1000) as f64,
                        rand_range(&mut state, -1000, 1000) as f64,
                        rand_range(&mut state, -1000, 1000) as f64,
                    ],
                })
                .collect();
            index.rebuild(&entities);
            assert_eq!(index.len(), n);

            let mut got: Vec<usize> = Vec::new();
            index.query_squared([0.0; 3], f64::INFINITY, |i| got.push(i));
            got.sort_unstable();
            assert_eq!(got, (0..n).collect::<Vec<_>>());
        }
    }
}
