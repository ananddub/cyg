use auto_di::singleton;

pub mod delete;
pub mod index;
pub mod list;
pub mod read;
pub mod write;

pub struct StorageService;

#[singleton]
impl StorageService {
    fn new() -> Self {
        Self
    }
}

