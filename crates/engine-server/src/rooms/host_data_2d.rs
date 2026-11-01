use engine_physics::RapierWorld2D;
use rapier2d::prelude as r2d;

use super::RoomHostData;

impl RoomHostData {

    pub fn init_rapier2d(&mut self, gx: f32, gy: f32) {
        if self.rapier2d.arena_width > 0.0 || self.rapier2d.collider_set.len() > 0 {
            self.rapier2d.gravity = r2d::Vector::new(gx, gy);
            return;
        }
        self.rapier2d = RapierWorld2D::new(gx, gy);
        self.rapier2d_bodies.clear();
        self.rapier2d_colliders.clear();
        self.rapier2d_next_body_id = 1;
        self.rapier2d_next_col_id = 1;
    }

    pub fn rapier2d_create_body(&mut self, body_type: i32, x: f32, y: f32) -> i32 {
        let handle = self.rapier2d.create_body(body_type, x, y);
        let id = self.rapier2d_next_body_id;
        self.rapier2d_next_body_id += 1;
        self.rapier2d_bodies.insert(id, handle);
        id
    }

    pub fn rapier2d_add_circle_collider(
        &mut self,
        body_id: i32,
        radius: f32,
        is_sensor: bool,
        restitution: f32,
        friction: f32,
    ) -> i32 {
        let Some(&handle) = self.rapier2d_bodies.get(&body_id) else {
            return 0;
        };
        let col_handle =
            self.rapier2d
                .add_circle_collider(handle, radius, is_sensor, restitution, friction);
        let col_id = self.rapier2d_next_col_id;
        self.rapier2d_next_col_id += 1;
        self.rapier2d_colliders.insert(col_id, col_handle);
        col_id
    }

    pub fn rapier2d_add_box_collider(
        &mut self,
        body_id: i32,
        half_w: f32,
        half_h: f32,
        is_sensor: bool,
        restitution: f32,
        friction: f32,
    ) -> i32 {
        let Some(&handle) = self.rapier2d_bodies.get(&body_id) else {
            return 0;
        };
        let col_handle = self.rapier2d.add_box_collider(
            handle,
            half_w,
            half_h,
            is_sensor,
            restitution,
            friction,
        );
        let col_id = self.rapier2d_next_col_id;
        self.rapier2d_next_col_id += 1;
        self.rapier2d_colliders.insert(col_id, col_handle);
        col_id
    }

    pub fn rapier2d_set_velocity(&mut self, body_id: i32, vx: f32, vy: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.set_velocity(handle, vx, vy);
        }
    }

    pub fn rapier2d_get_velocity(&self, body_id: i32) -> (f32, f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.get_velocity(handle)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn rapier2d_get_position(&self, body_id: i32) -> Option<(f32, f32)> {
        let &handle = self.rapier2d_bodies.get(&body_id)?;
        self.rapier2d.get_position(handle)
    }

    pub fn rapier2d_set_position(&mut self, body_id: i32, x: f32, y: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.set_position(handle, x, y);
        }
    }

    pub fn rapier2d_apply_impulse(&mut self, body_id: i32, ix: f32, iy: f32) {
        if let Some(&handle) = self.rapier2d_bodies.get(&body_id) {
            self.rapier2d.apply_impulse(handle, ix, iy);
        }
    }

    pub fn rapier2d_step(&mut self, dt: f32) {
        if dt > 0.0 {
            self.rapier2d.integration_parameters.dt = dt;
        }
        self.rapier2d.step();
    }

    pub fn rapier2d_check_contact(&self, body_id_a: i32, body_id_b: i32) -> bool {
        let (Some(&handle_a), Some(&handle_b)) = (
            self.rapier2d_bodies.get(&body_id_a),
            self.rapier2d_bodies.get(&body_id_b),
        ) else {
            return false;
        };
        self.rapier2d.check_intersection(handle_a, handle_b)
    }

    pub fn rapier2d_remove_body(&mut self, body_id: i32) {
        if let Some(handle) = self.rapier2d_bodies.remove(&body_id) {
            self.rapier2d.remove_body(handle);
        }
    }

    pub fn load_arena_2d(&mut self, map: &engine_physics::MapConfig2D) {
        let json = serde_json::to_string(map).expect("MapConfig2D serializes");
        if let Ok(world) = RapierWorld2D::from_json(&json) {
            let (gw, gh) = (world.gravity.x, world.gravity.y);
            let arena = (world.arena_width, world.arena_height);
            let counters = (self.rapier2d_next_body_id, self.rapier2d_next_col_id);
            self.rapier2d = world;
            self.rapier2d.gravity = r2d::Vector::new(gw, gh);
            self.rapier2d.arena_width = arena.0;
            self.rapier2d.arena_height = arena.1;
            self.rapier2d_next_body_id = counters.0;
            self.rapier2d_next_col_id = counters.1;
        }
    }

}
