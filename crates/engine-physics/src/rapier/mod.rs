mod ecs_sync;
mod map2d;
mod world2d;
mod world2d_ops;
mod world2d_query;
mod world3d;
mod world3d_query;

#[cfg(test)]
mod tests;

pub use ecs_sync::{
    step_rapier_2d, step_rapier_3d, sync_ecs_to_rapier_2d, sync_ecs_to_rapier_3d,
    sync_rapier_to_ecs_2d, sync_rapier_to_ecs_3d, sync_velocity_to_rapier_2d,
    sync_velocity_to_rapier_3d,
};
pub use map2d::{MapConfig2D, MapObstacleDef2D};
pub use world2d::RapierWorld2D;
pub use world3d::RapierWorld3D;
