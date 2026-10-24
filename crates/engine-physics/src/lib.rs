//! # OpenOxide Physics Engine
//!
//! Provides deterministic 3D AABB obstacle collisions, Kinematic Character
//! Controllers (move_and_slide), Continuous Collision Detection (CCD)
//! raycasts, lag compensation rewind hit-testing, and Rapier 2D/3D physics worlds.

pub mod collision;
pub mod fixed;
pub mod oxcl;
pub mod rapier;

pub use collision::*;
pub use fixed::*;
pub use oxcl::*;
pub use rapier::*;
pub use rapier2d;
pub use rapier3d;
