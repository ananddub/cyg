use rapier3d::prelude as r3d;

impl super::RapierWorld3D {
    pub fn check_intersection(
        &self,
        body_a: r3d::RigidBodyHandle,
        body_b: r3d::RigidBodyHandle,
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

    pub fn set_velocity(&mut self, handle: r3d::RigidBodyHandle, vx: f32, vy: f32, vz: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.set_linvel(r3d::Vector::new(vx, vy, vz), true);
        }
    }

    pub fn velocity(&self, handle: r3d::RigidBodyHandle) -> [f32; 3] {
        match self.rigid_body_set.get(handle) {
            Some(body) => {
                let v = body.linvel();
                [v.x, v.y, v.z]
            }
            None => [0.0; 3],
        }
    }

    pub fn position(&self, handle: r3d::RigidBodyHandle) -> [f32; 3] {
        match self.rigid_body_set.get(handle) {
            Some(body) => {
                let t = body.translation();
                [t.x, t.y, t.z]
            }
            None => [0.0; 3],
        }
    }

    pub fn raycast_hit(
        &mut self,
        origin: [f32; 3],
        dir: [f32; 3],
        max_dist: f32,
    ) -> Option<(u32, f32)> {
        let ray = r3d::Ray::new(
            r3d::Vector::new(origin[0], origin[1], origin[2]),
            r3d::Vector::new(dir[0], dir[1], dir[2]),
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
