use auto_di::singleton;

pub mod assign;
mod resolve;

pub use resolve::is_on;

pub struct FlagService;

#[singleton]
impl FlagService {
    fn new() -> Self {
        Self
    }
}
