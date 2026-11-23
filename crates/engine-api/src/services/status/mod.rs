use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod readyz;
pub mod rooms;
pub mod snapshot;

#[cfg(test)]
mod tests;

pub struct StatusService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl StatusService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
