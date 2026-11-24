use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

mod add;
mod list;
mod remove;
mod roles;
mod users;

pub struct GroupMemberService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl GroupMemberService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
