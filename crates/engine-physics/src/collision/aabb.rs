use serde::{Deserialize, Serialize};

use super::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[inline]
    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        let half = size * 0.5;
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    #[inline]
    pub fn contains_point(&self, pt: Vec3) -> bool {
        pt.x >= self.min.x
            && pt.x <= self.max.x
            && pt.y >= self.min.y
            && pt.y <= self.max.y
            && pt.z >= self.min.z
            && pt.z <= self.max.z
    }

    #[inline]
    pub fn closest_point(&self, pt: Vec3) -> Vec3 {
        Vec3::new(
            pt.x.max(self.min.x).min(self.max.x),
            pt.y.max(self.min.y).min(self.max.y),
            pt.z.max(self.min.z).min(self.max.z),
        )
    }

    pub fn ray_intersect(&self, origin: Vec3, dir: Vec3, max_dist: f64) -> Option<RayHit> {
        let mut tmin = 0.0;
        let mut tmax = max_dist;
        let mut normal = Vec3::ZERO;

        if dir.x.abs() > 1e-8 {
            let inv_d = 1.0 / dir.x;
            let mut t1 = (self.min.x - origin.x) * inv_d;
            let mut t2 = (self.max.x - origin.x) * inv_d;
            let mut n = Vec3::new(-1.0, 0.0, 0.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(1.0, 0.0, 0.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.x < self.min.x || origin.x > self.max.x {
            return None;
        }

        if dir.y.abs() > 1e-8 {
            let inv_d = 1.0 / dir.y;
            let mut t1 = (self.min.y - origin.y) * inv_d;
            let mut t2 = (self.max.y - origin.y) * inv_d;
            let mut n = Vec3::new(0.0, -1.0, 0.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(0.0, 1.0, 0.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.y < self.min.y || origin.y > self.max.y {
            return None;
        }

        if dir.z.abs() > 1e-8 {
            let inv_d = 1.0 / dir.z;
            let mut t1 = (self.min.z - origin.z) * inv_d;
            let mut t2 = (self.max.z - origin.z) * inv_d;
            let mut n = Vec3::new(0.0, 0.0, -1.0);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = Vec3::new(0.0, 0.0, 1.0);
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        } else if origin.z < self.min.z || origin.z > self.max.z {
            return None;
        }

        if tmin <= max_dist && tmin >= 0.0 {
            let point = origin + dir * tmin;
            Some(RayHit {
                hit: true,
                point,
                normal,
                distance: tmin,
                collider_id: 0,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RayHit {
    pub hit: bool,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f64,
    pub collider_id: u32,
}
