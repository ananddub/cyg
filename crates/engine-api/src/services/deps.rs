//! Resolving the optional data layer.
//!
//! Two call shapes over one rule: a service that holds its own `Arc<Plane>`
//! asks `plane.q()`; a free function that already has a `&Plane` calls `q(plane)`.
//! Both first ask the request's DI container, so a test that registers a
//! datastore is honoured, and both fall back to the plane's own handle.

use std::sync::Arc;

use crate::db::BlobStore;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::services::plane::Plane;

pub(crate) async fn q(plane: &Plane) -> Result<Arc<Queries>, V2Error> {
    plane.store.q().await
}

/// The datastore, from the DI container when it is registered and from
/// `fallback` otherwise. The second half is what keeps a service constructible
/// in a test that installs a plane but no container.
pub(crate) async fn queries(fallback: Option<Arc<Queries>>) -> Result<Arc<Queries>, V2Error> {
    match crate::app::state::resolve::<Queries>().await {
        Ok(queries) => Ok(queries),
        Err(_) => fallback.ok_or_else(V2Error::no_db),
    }
}

pub(crate) async fn blobs(plane: &Plane) -> Result<Arc<BlobStore>, V2Error> {
    plane.store.blobs().await
}

pub(crate) async fn blob_store(
    fallback: Option<Arc<BlobStore>>,
) -> Result<Arc<BlobStore>, V2Error> {
    match crate::app::state::resolve::<BlobStore>().await {
        Ok(blobs) => Ok(blobs),
        Err(_) => fallback.ok_or_else(V2Error::no_blobs),
    }
}
