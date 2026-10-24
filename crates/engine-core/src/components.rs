use bevy_ecs::prelude::*;

/// Unique game entity identifier across ticks and clients
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityId(pub u32);

/// 2D plane position (x, y)
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Position2D {
    pub x: f32,
    pub y: f32,
}

/// 3D space position (x, y, z)
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Position3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Linear velocity vector
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

/// Identifies an entity controlled by an authoritative client session
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerControlled {
    pub client_id: u64,
}

/// Vitality component for gameplay combat and mechanics
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
        self.current = (self.current - amount).max(0.0);
        self.is_dead()
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

/// Marker component for cleanup at the end of the simulation tick
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PendingDespawn;
