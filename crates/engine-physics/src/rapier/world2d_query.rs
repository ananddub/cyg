use rapier2d::prelude as r2d;

impl super::RapierWorld2D {
    pub fn get_velocity(&self, body: r2d::RigidBodyHandle) -> (f32, f32) {
        if let Some(rb) = self.rigid_body_set.get(body) {
            let v = rb.linvel();
            (v.x, v.y)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn get_position(&self, body: r2d::RigidBodyHandle) -> Option<(f32, f32)> {
        self.rigid_body_set.get(body).map(|rb| {
            let t = rb.translation();
            (t.x, t.y)
        })
    }

    pub fn check_intersection(
        &self,
        body_a: r2d::RigidBodyHandle,
        body_b: r2d::RigidBodyHandle,
    ) -> bool {
        let (Some(rb_a), Some(rb_b)) = (
            self.rigid_body_set.get(body_a),
            self.rigid_body_set.get(body_b),
        ) else {
            return false;
        };
        for &col_a in rb_a.colliders() {
            for &col_b in rb_b.colliders() {
                if self.narrow_phase.intersection_pair(col_a, col_b) == Some(true) {
                    return true;
                }

                for pair in self.narrow_phase.contact_pairs_with(col_a) {
                    let matched = if pair.collider1 == col_a {
                        pair.collider2 == col_b
                    } else if pair.collider2 == col_a {
                        pair.collider1 == col_b
                    } else {
                        continue;
                    };
                    if matched {
                        return pair.has_any_active_contact();
                    }
                }
            }
        }
        false
    }

    pub fn raycast_hit(
        &mut self,
        origin: [f32; 2],
        dir: [f32; 2],
        max_dist: f32,
    ) -> Option<(u32, f32)> {
        let ray = r2d::Ray::new(
            r2d::Vector::new(origin[0], origin[1]),
            r2d::Vector::new(dir[0], dir[1]),
        );

        let mut best: Option<(u32, f32)> = None;
        for (_handle, col) in self.collider_set.iter() {
            let entity = match col.parent().and_then(|p| self.rigid_body_set.get(p)) {
                Some(_) => self
                    .handle_entities
                    .get(&col.parent().expect("checked above"))
                    .copied()
                    .unwrap_or(0),
                None => col.user_data as u32,
            };
            let world_iso = *col.position();
            let local_ray = ray.transform_by(&world_iso.inverse());
            let Some(toi) = col.shape().cast_local_ray(&local_ray, max_dist, true) else {
                continue;
            };
            if best.as_ref().map_or(true, |(_, d)| toi < *d) {
                best = Some((entity, toi));
            }
        }
        best
    }
}
