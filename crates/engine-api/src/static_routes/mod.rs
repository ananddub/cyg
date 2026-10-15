//! Static assets for the control plane.
//!
//! Replaces the in-transport `on_http` hook: the website page, the SDK bundle,
//! the arena map, the Unity WebGL build and the OpenOxide SDK files are served
//! from here, on the control port, with an mtime-keyed cache so a reload during
//! development picks up a new `openoxide.js` without a restart.
//!
//! Every path that reaches the filesystem goes through
//! [`StaticServer::resolve_under`], which rejects anything that is not a plain
//! child of the root it was given — no `..`, no absolute path, no escape via
//! percent-decoding. Two roots exist: the project directory (pages and their
//! assets) and the Unity WebGL build directory (a fixed tree the page
//! references with root-absolute `/Build/…` and `/TemplateData/…` URLs).

mod cache;
mod guard;
mod handlers;
mod mime;
mod routes;
mod server;

pub use mime::content_type_for;
pub use routes::{fallback, router};
pub use server::StaticServer;

#[cfg(test)]
mod tests;
