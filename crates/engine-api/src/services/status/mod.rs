use auto_di::singleton;

pub mod rooms;
pub mod readyz;
pub mod snapshot;

#[cfg(test)]
mod tests;

pub struct StatusService;

#[singleton]
impl StatusService {
    fn new() -> Self {
        Self
    }
}
