use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod chain;
pub mod handoff;
pub mod reserve;

#[cfg(test)]
mod tests;

pub struct MatchmakeService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl MatchmakeService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
