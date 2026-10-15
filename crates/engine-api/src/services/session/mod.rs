//! Session lifecycle: `POST /v2/account/session/refresh` and `POST /v2/session/logout`.
//!
//! A refresh carries no still-valid session, so the gateway proves the *caller* with the
//! server key; a logout holds a live session, so it is a bearer route. Both are `body:"*"`.

use auto_di::singleton;

pub mod logout;
pub mod refresh;

pub struct SessionService;

#[singleton]
impl SessionService {
    fn new() -> Self {
        Self
    }
}
