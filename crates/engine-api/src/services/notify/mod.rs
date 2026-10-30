use auto_di::singleton;

pub mod outbox;
pub(crate) mod preferences;
pub(crate) mod worker;

pub use worker::delivery_loop;

pub struct NotifyService;

#[singleton]
impl NotifyService {
    fn new() -> Self {
        Self
    }
}
