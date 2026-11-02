pub mod aoi;

pub use aoi::{DeltaEntity, filter_for_viewer, parse_delta, viewer_pos_of};

mod aoi2d;
mod aoi3d;
mod index2d;
mod index3d;
mod types;

pub use aoi2d::AoiManager2D;
pub use aoi3d::AoiManager3D;
pub use index2d::SpatialIndex2D;
pub use index3d::SpatialIndex3D;
pub use types::{SpatialQueryResult, VisibilityDelta};

#[cfg(test)]
mod tests;
