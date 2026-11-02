use super::Aabb;

const MAX_GRID_CANDIDATES: usize = 128;

pub(super) struct Candidates {
    ids: [u32; MAX_GRID_CANDIDATES],
    len: usize,

    pub(super) overflow: bool,
}

impl Candidates {
    #[inline]
    fn new() -> Self {
        Self {
            ids: [0; MAX_GRID_CANDIDATES],
            len: 0,
            overflow: false,
        }
    }

    #[inline]
    fn add(&mut self, id: u32) {
        if self.overflow {
            return;
        }
        let len = self.len;
        if len == self.ids.len() {
            self.overflow = true;
            return;
        }
        let pos = self.ids[..len].partition_point(|&x| x < id);
        if pos < len && self.ids[pos] == id {
            return;
        }
        self.ids.copy_within(pos..len, pos + 1);
        self.ids[pos] = id;
        self.len = len + 1;
    }

    #[inline]
    pub(super) fn as_slice(&self) -> &[u32] {
        &self.ids[..self.len]
    }
}

#[derive(Debug, Clone)]
pub(super) struct StaticObstacleGrid {
    cell_size: f64,
    half_x: f64,
    half_z: f64,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

impl StaticObstacleGrid {
    pub(super) fn new(half_x: f64, half_z: f64) -> Self {
        let extent = (half_x.max(half_z) * 2.0).max(1.0);
        let cell_size = 16.0f64.max(extent / 256.0);
        let cols = ((half_x * 2.0) / cell_size).ceil() as usize + 1;
        let rows = ((half_z * 2.0) / cell_size).ceil() as usize + 1;
        Self {
            cell_size,
            half_x,
            half_z,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
        }
    }

    #[inline]
    fn col_of(&self, x: f64) -> usize {
        (((x + self.half_x) / self.cell_size).max(0.0) as usize).min(self.cols - 1)
    }

    #[inline]
    fn row_of(&self, z: f64) -> usize {
        (((z + self.half_z) / self.cell_size).max(0.0) as usize).min(self.rows - 1)
    }

    pub(super) fn insert(&mut self, obs_index: usize, bounds: &Aabb) {
        let id = obs_index as u32;
        let (c0, c1) = (self.col_of(bounds.min.x), self.col_of(bounds.max.x));
        let (r0, r1) = (self.row_of(bounds.min.z), self.row_of(bounds.max.z));
        for row in r0..=r1 {
            let start = row * self.cols;
            for col in c0..=c1 {
                self.cells[start + col].push(id);
            }
        }
    }

    pub(super) fn query(&self, min_x: f64, max_x: f64, min_z: f64, max_z: f64) -> Candidates {
        let mut out = Candidates::new();
        let (lo_x, hi_x) = (min_x.min(max_x), min_x.max(max_x));
        let (lo_z, hi_z) = (min_z.min(max_z), min_z.max(max_z));
        let (c0, c1) = (self.col_of(lo_x), self.col_of(hi_x));
        let (r0, r1) = (self.row_of(lo_z), self.row_of(hi_z));
        for row in r0..=r1 {
            let start = row * self.cols;
            for col in c0..=c1 {
                for &id in &self.cells[start + col] {
                    out.add(id);
                }
            }
        }
        out
    }
}
