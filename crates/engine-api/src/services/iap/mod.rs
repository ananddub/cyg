use auto_di::singleton;

pub mod purchases;
pub mod subscriptions;
pub mod vendors;

pub struct IapService;

#[singleton]
impl IapService {
    fn new() -> Self {
        Self
    }
}
