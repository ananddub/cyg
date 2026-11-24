use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod files;
pub mod server;

mod cache;

pub use server::StaticServer;

#[cfg(test)]
mod tests;

pub struct StaticRoutesService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl StaticRoutesService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
