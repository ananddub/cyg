//! The group membership service: listing, adding, kicking, banning, promoting
//! and demoting members, all expressed as `group_edge` rows.

use auto_di::singleton;

mod add;
mod list;
mod remove;
mod roles;
mod users;

pub struct GroupMemberService;

#[singleton]
impl GroupMemberService {
    fn new() -> Self {
        Self
    }
}
