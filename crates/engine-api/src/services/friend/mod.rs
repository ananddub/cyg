use auto_di::singleton;

mod imports;
mod list;
mod mutate;

pub struct FriendService;

#[singleton]
impl FriendService {
    fn new() -> Self {
        Self
    }
}
