use super::RoomHostData;

impl RoomHostData {
    pub fn init_rapier3d(&mut self, gx: f32, gy: f32, gz: f32) {
        if !self.features.wants_rapier3d() {
            return; // feature off — the world must never be resurrected
        }
        self.rapier3d = Some(engine_physics::RapierWorld3D::new(gx, gy, gz));
        self.rapier3d_bodies.clear();
        self.rapier3d_colliders.clear();
        self.rapier3d_next_body_id = 1;
        self.rapier3d_next_col_id = 1;
    }

    pub fn rapier3d_create_body(&mut self, body_type: i32, x: f32, y: f32, z: f32) -> i32 {
        let Some(world) = self.rapier3d.as_mut() else {
            return 0;
        };
        let handle = world.create_body(body_type, x, y, z);
        let id = self.rapier3d_next_body_id;
        self.rapier3d_next_body_id += 1;
        self.rapier3d_bodies.insert(id, handle);
        id
    }

    pub fn rapier3d_add_ball_collider(&mut self, body_id: i32, radius: f32) -> i32 {
        let Some(&handle) = self.rapier3d_bodies.get(&body_id) else {
            return 0;
        };
        let Some(world) = self.rapier3d.as_mut() else {
            return 0;
        };
        let col = world.add_ball_collider(handle, radius);
        let id = self.rapier3d_next_col_id;
        self.rapier3d_next_col_id += 1;
        self.rapier3d_colliders.insert(id, col);
        id
    }

    pub fn rapier3d_add_box_collider(&mut self, body_id: i32, hx: f32, hy: f32, hz: f32) -> i32 {
        let Some(&handle) = self.rapier3d_bodies.get(&body_id) else {
            return 0;
        };
        let Some(world) = self.rapier3d.as_mut() else {
            return 0;
        };
        let col = world.add_box_collider(handle, hx, hy, hz);
        let id = self.rapier3d_next_col_id;
        self.rapier3d_next_col_id += 1;
        self.rapier3d_colliders.insert(id, col);
        id
    }

    pub fn rapier3d_set_position(&mut self, body_id: i32, x: f32, y: f32, z: f32) {
        if let (Some(&handle), Some(world)) =
            (self.rapier3d_bodies.get(&body_id), self.rapier3d.as_mut())
        {
            world.set_position(handle, x, y, z);
        }
    }

    pub fn rapier3d_set_velocity(&mut self, body_id: i32, vx: f32, vy: f32, vz: f32) {
        if let (Some(&handle), Some(world)) =
            (self.rapier3d_bodies.get(&body_id), self.rapier3d.as_mut())
        {
            world.set_velocity(handle, vx, vy, vz);
        }
    }

    pub fn rapier3d_get_position(&self, body_id: i32) -> Option<[f32; 3]> {
        let &handle = self.rapier3d_bodies.get(&body_id)?;
        Some(self.rapier3d.as_ref()?.position(handle))
    }

    pub fn rapier3d_get_velocity(&self, body_id: i32) -> [f32; 3] {
        match (self.rapier3d_bodies.get(&body_id), self.rapier3d.as_ref()) {
            (Some(&handle), Some(world)) => world.velocity(handle),
            _ => [0.0; 3],
        }
    }

    pub fn rapier3d_apply_impulse(&mut self, body_id: i32, ix: f32, iy: f32, iz: f32) {
        if let (Some(&handle), Some(world)) =
            (self.rapier3d_bodies.get(&body_id), self.rapier3d.as_mut())
        {
            world.apply_impulse(handle, ix, iy, iz);
        }
    }

    pub fn rapier3d_step(&mut self, dt: f32) {
        let Some(world) = self.rapier3d.as_mut() else {
            return;
        };
        if dt > 0.0 {
            world.integration_parameters.dt = dt;
        }
        world.step();
    }

    pub fn rapier3d_check_contact(&self, body_id_a: i32, body_id_b: i32) -> bool {
        match (
            self.rapier3d_bodies.get(&body_id_a),
            self.rapier3d_bodies.get(&body_id_b),
            self.rapier3d.as_ref(),
        ) {
            (Some(&a), Some(&b), Some(world)) => world.check_intersection(a, b),
            _ => false,
        }
    }

    pub fn rapier3d_remove_body(&mut self, body_id: i32) {
        if let (Some(handle), Some(world)) = (
            self.rapier3d_bodies.remove(&body_id),
            self.rapier3d.as_mut(),
        ) {
            world.remove_body(handle);
        }
    }
}
