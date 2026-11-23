use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod delete;
pub mod export;
pub mod read;
pub mod update;
pub mod wallet;

pub struct AccountService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl AccountService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
