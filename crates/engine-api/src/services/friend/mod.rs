//! Friends and blocking (`/v2/friend*`).
//!
//! BaaS parity: `server/api_friend.go`, `core_friend.go`. `user_edge` is a bidirectional
//! graph (`0=Friends..3=Blocked`); adding is an *invite*, until the other side adds back.

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
