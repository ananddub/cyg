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
