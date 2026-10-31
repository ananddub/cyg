use auto_di::singleton;

pub mod files;
pub mod server;

mod cache;

pub use server::StaticServer;

#[cfg(test)]
mod tests;

pub struct StaticRoutesService;

#[singleton]
impl StaticRoutesService {
    fn new() -> Self {
        Self
    }
}
