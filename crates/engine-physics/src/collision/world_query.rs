use super::{RayHit, SlideResult, Vec3};

const MAX_SLIDE_PASSES: usize = 8;

impl super::PhysicsWorld3D {
    pub fn get_surface_height(&self, px: f64, py: f64, pz: f64, radius: f64) -> f64 {
        let mut surface_y = 0.0;
        let r = radius * 0.85;

        self.for_each_in_rect(px - r, px + r, pz - r, pz + r, |obs| {
            if !obs.is_walkable_roof {
                return;
            }
            let min_x = obs.bounds.min.x - r;
            let max_x = obs.bounds.max.x + r;
            let min_z = obs.bounds.min.z - r;
            let max_z = obs.bounds.max.z + r;

            if px >= min_x && px <= max_x && pz >= min_z && pz <= max_z {
                if py >= obs.height - 2.5 && obs.height > surface_y {
                    surface_y = obs.height;
                }
            }
        });

        surface_y
    }

    pub fn move_and_slide(
        &self,
        px: f64,
        py: f64,
        pz: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        radius: f64,
        dt: f64,
    ) -> SlideResult {
        let mut cur_x = px + vx * dt;
        let mut cur_y = py;
        let mut cur_z = pz + vz * dt;
        let mut is_walled = false;

        let probe = radius * 2.0;
        for _ in 0..MAX_SLIDE_PASSES {
            let before = (cur_x, cur_z);
            self.for_each_in_rect(
                cur_x - probe,
                cur_x + probe,
                cur_z - probe,
                cur_z + probe,
                |obs| {
                    if cur_y >= obs.height - 1.0 {
                        return;
                    }
                    let cx = cur_x.max(obs.bounds.min.x).min(obs.bounds.max.x);
                    let cz = cur_z.max(obs.bounds.min.z).min(obs.bounds.max.z);
                    let dx = cur_x - cx;
                    let dz = cur_z - cz;
                    let dist_sq = dx * dx + dz * dz;

                    if dist_sq < radius * radius {
                        is_walled = true;
                        let dist = dist_sq.sqrt();
                        if dist > 1e-4 {
                            let overlap = radius - dist;
                            cur_x += (dx / dist) * overlap;
                            cur_z += (dz / dist) * overlap;
                        } else {
                            let dl = cur_x - obs.bounds.min.x;
                            let dr = obs.bounds.max.x - cur_x;
                            let dt_z = cur_z - obs.bounds.min.z;
                            let db_z = obs.bounds.max.z - cur_z;
                            let min_d = dl.min(dr).min(dt_z).min(db_z);

                            if (min_d - dl).abs() < 1e-6 {
                                cur_x = obs.bounds.min.x - radius;
                            } else if (min_d - dr).abs() < 1e-6 {
                                cur_x = obs.bounds.max.x + radius;
                            } else if (min_d - dt_z).abs() < 1e-6 {
                                cur_z = obs.bounds.min.z - radius;
                            } else {
                                cur_z = obs.bounds.max.z + radius;
                            }
                        }
                    }
                },
            );
            if (cur_x, cur_z) == before {
                break;
            }
        }

        let bound_x = self.arena_half_width - radius;
        let bound_z = self.arena_half_depth - radius;
        cur_x = cur_x.max(-bound_x).min(bound_x);
        cur_z = cur_z.max(-bound_z).min(bound_z);

        let surface_y = self.get_surface_height(cur_x, cur_y, cur_z, radius);
        let mut is_grounded = false;

        if vy <= 0.0 && cur_y <= surface_y + 0.1 {
            cur_y = surface_y;
            is_grounded = true;
        }

        SlideResult {
            new_x: cur_x,
            new_y: cur_y,
            new_z: cur_z,
            is_grounded,
            is_walled,
        }
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f64) -> Option<RayHit> {
        let norm_dir = dir.normalize();
        let mut closest_hit: Option<RayHit> = None;
        let mut nearest_dist = max_dist;

        let far = origin + norm_dir * max_dist;
        self.for_each_in_rect(origin.x, far.x, origin.z, far.z, |obs| {
            if let Some(mut hit) = obs.bounds.ray_intersect(origin, norm_dir, nearest_dist) {
                if hit.distance < nearest_dist {
                    nearest_dist = hit.distance;
                    hit.collider_id = obs.id;
                    closest_hit = Some(hit);
                }
            }
        });

        closest_hit
    }
}
