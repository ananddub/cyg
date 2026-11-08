#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformSnapshot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y: f32,
    pub hit_radius: f32,
}

impl TransformSnapshot {
    #[inline]
    pub fn new(x: f32, y: f32, z: f32, hit_radius: f32) -> Self {
        Self {
            x,
            y,
            z,
            rotation_y: 0.0,
            hit_radius,
        }
    }

    #[inline]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
            rotation_y: self.rotation_y + (other.rotation_y - self.rotation_y) * t,
            hit_radius: self.hit_radius + (other.hit_radius - self.hit_radius) * t,
        }
    }

    #[inline]
    pub fn is_point_hit(&self, px: f32, py: f32, pz: f32) -> bool {
        let dx = self.x - px;
        let dy = self.y - py;
        let dz = self.z - pz;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= (self.hit_radius * self.hit_radius)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    #[default]
    Linear,

    Step,
}

#[derive(Debug, Clone, Copy)]
pub struct HistorySample<T> {
    pub tick: u64,
    pub time_ms: f64,
    pub data: T,
}

pub(super) struct Bounding<T> {
    pub(super) prev: HistorySample<T>,
    pub(super) next: HistorySample<T>,
    pub(super) t: f32,
}
