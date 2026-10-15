//! User profiles (`/v2/user`).
//!
//! BaaS parity: `server/api_user.go`, `core_user.go`. The read-only public view of *other*
//! accounts: unknown or tombstoned ones are simply absent (the multi-get never fails on a miss).

use auto_di::singleton;

pub mod lookup;

pub struct UserService;

#[singleton]
impl UserService {
    fn new() -> Self {
        Self
    }
}
