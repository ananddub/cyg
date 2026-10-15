#![doc = include_str!("../README.md")]

pub use auto_route_macros::{controller, delete, get, head, options, patch, post, put};

mod client;
mod docs;
mod openapi;
mod openapi_descriptors;
mod router;

pub use client::{
    LiveClientArgumentDescriptor, LiveClientRouteDescriptor, live_client_manifest, ts_decl, ts_name,
};
pub use docs::{openapi_routes, scalar_routes};
pub use openapi::{AutoRouteOpenApi, openapi_json, openapi_spec};
pub use openapi_descriptors::{
    OpenApiParamDescriptor, OpenApiRouteDescriptor, OpenApiSchemaDescriptor,
};
pub use router::{RouteDescriptor, build_routes, routes};

#[doc(hidden)]
pub mod __private {
    pub use auto_di;
    pub use auto_socket;
    pub use axum;
    pub use inventory;
    pub use poem_openapi;
    pub use serde_json;
    pub use tracing;
}
