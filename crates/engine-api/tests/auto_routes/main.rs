use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

mod every_migrated_path_is_still_registered;
mod only_v1_is_versioned;
mod spec_covers_pinned_routes;
mod state;

pub use every_migrated_path_is_still_registered::*;
pub use spec_covers_pinned_routes::*;
pub use state::*;
