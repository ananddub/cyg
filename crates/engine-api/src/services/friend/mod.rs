use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

mod imports;
mod list;
mod mutate;

pub struct FriendService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl FriendService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
