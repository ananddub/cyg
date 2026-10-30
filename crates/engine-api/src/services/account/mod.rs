use auto_di::singleton;

pub mod delete;
pub mod export;
pub mod read;
pub mod update;
pub mod wallet;

pub struct AccountService;

#[singleton]
impl AccountService {
    fn new() -> Self {
        Self
    }
}
