use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod lookup;

pub struct UserService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl UserService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
