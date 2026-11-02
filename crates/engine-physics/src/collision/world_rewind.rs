use super::lag::{HistoricalEntity, LagCompensatedHit};
use super::{Aabb, Vec3};

impl super::PhysicsWorld3D {
    pub fn record_entity_hitbox(
        &mut self,
        tick: u64,
        timestamp_ms: u64,
        entity_id: u32,
        kind: u32,
        bounds: Aabb,
    ) {
        self.lag_comp.record_entity(
            tick,
            timestamp_ms,
            HistoricalEntity {
                entity_id,
                kind,
                bounds,
            },
        );
    }

    pub fn lag_compensated_raycast(
        &mut self,
        shoot_tick: u64,
        origin: Vec3,
        dir: Vec3,
        max_dist: f64,
    ) -> Option<LagCompensatedHit> {
        let norm_dir = dir.normalize();

        let static_hit = self.raycast(origin, norm_dir, max_dist);
        let max_range = match &static_hit {
            Some(h) => h.distance,
            None => max_dist,
        };

        let snapshot = self.lag_comp.find_snapshot(shoot_tick)?;

        let mut closest_entity: Option<LagCompensatedHit> = None;
        let mut nearest_dist = max_range;

        for ent in &snapshot.entities {
            if let Some(hit) = ent.bounds.ray_intersect(origin, norm_dir, nearest_dist) {
                if hit.distance < nearest_dist {
                    nearest_dist = hit.distance;
                    closest_entity = Some(LagCompensatedHit {
                        point: hit.point,
                        distance: hit.distance,
                        entity_id: ent.entity_id,
                        kind: ent.kind,
                        collider_id: 0,
                        hit_type: 1,
                    });
                }
            }
        }

        if let Some(ent_hit) = closest_entity {
            return Some(ent_hit);
        }

        if let Some(wall) = static_hit {
            return Some(LagCompensatedHit {
                point: wall.point,
                distance: wall.distance,
                entity_id: 0,
                kind: 0,
                collider_id: wall.collider_id,
                hit_type: 2,
            });
        }

        None
    }
}
