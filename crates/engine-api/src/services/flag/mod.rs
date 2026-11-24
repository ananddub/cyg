use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod assign;
mod resolve;

pub use resolve::is_on;

pub struct FlagService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl FlagService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
