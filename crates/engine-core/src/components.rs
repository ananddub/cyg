use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityId(pub u32);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Position2D {
    pub x: f32,
    pub y: f32,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Position3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerControlled {
    pub client_id: u64,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn take_damage(&mut self, amount: f32) -> bool {
        self.current = (self.current - amount).clamp(0.0, self.max);
        self.is_dead()
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PendingDespawn;
