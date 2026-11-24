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
pub use openapi::{AutoRouteOpenApi, openapi_json, openapi_spec, openapi_spec_at, set_api_base};
pub use openapi_descriptors::{
    OpenApiParamDescriptor, OpenApiRouteDescriptor, OpenApiSchemaDescriptor, documented_routes,
};
pub use router::{
    RouteDescriptor, build_root_routes, build_routes, build_routes_at, build_versioned_routes,
    routes,
};

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
