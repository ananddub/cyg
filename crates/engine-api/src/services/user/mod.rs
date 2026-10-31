use auto_di::singleton;

pub mod lookup;

pub struct UserService;

#[singleton]
impl UserService {
    fn new() -> Self {
        Self
    }
}
