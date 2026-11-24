use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

mod create;
mod join;
mod read;
mod update;

const GROUP_OPEN: i16 = 0;
const GROUP_CLOSED: i16 = 1;

pub struct GroupService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl GroupService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
