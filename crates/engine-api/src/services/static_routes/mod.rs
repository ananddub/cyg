//! Static assets for the browser-facing API, served on the API port with an mtime-keyed cache.
//!
//! Every path to the filesystem goes through [`StaticServer::resolve_under`], which rejects
//! anything that is not a plain child of its root — no `..`, no escape via percent-decoding.

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
