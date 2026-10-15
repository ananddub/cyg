//! The group lifecycle service: create/get/update/delete, join/leave and the
//! `ListGroups` search.

use auto_di::singleton;

mod create;
mod join;
mod read;
mod update;

const GROUP_OPEN: i16 = 0;
const GROUP_CLOSED: i16 = 1;

pub struct GroupService;

#[singleton]
impl GroupService {
    fn new() -> Self {
        Self
    }
}
