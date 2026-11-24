use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod logout;
pub mod refresh;

pub struct SessionService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl SessionService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
