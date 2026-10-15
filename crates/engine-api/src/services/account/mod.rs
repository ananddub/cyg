//! `/v2/account` — the signed-in player's own record: read it, change it, delete it.
//!
//! BaaS parity: `server/api_account.go`. Credential attach/detach lives in
//! [`crate::services::link`] and [`crate::services::unlink`].

use auto_di::singleton;

pub mod delete;
pub mod read;
pub mod update;

pub struct AccountService;

#[singleton]
impl AccountService {
    fn new() -> Self {
        Self
    }
}
