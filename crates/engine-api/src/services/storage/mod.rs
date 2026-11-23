use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod delete;
pub mod index;
pub mod list;
pub mod read;
pub mod write;

pub struct StorageService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl StorageService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
