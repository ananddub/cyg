use rapier2d::prelude as r2d;

impl super::RapierWorld2D {
    pub fn set_velocity(&mut self, body: r2d::RigidBodyHandle, vx: f32, vy: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.set_linvel(r2d::Vector::new(vx, vy), true);
        }
    }

    pub fn set_position(&mut self, body: r2d::RigidBodyHandle, x: f32, y: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.set_translation(r2d::Vector::new(x, y), true);
        }
    }

    pub fn apply_impulse(&mut self, body: r2d::RigidBodyHandle, ix: f32, iy: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.apply_impulse(r2d::Vector::new(ix, iy), true);
        }
    }

    pub fn remove_body(&mut self, body: r2d::RigidBodyHandle) {
        self.rigid_body_set.remove(
            body,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            true,
        );
    }
}
