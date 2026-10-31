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
