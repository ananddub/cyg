//! What a service may inject instead of taking `&ApiState`.
//!
//! The plane's data dependencies are optional — a process with no database
//! configured still serves `/status`, `/matchmake` and the dashboard — so they
//! cannot be registered as instances: a controller that resolves `Queries`
//! would simply fail to mount. They are carried in one always-present handle
//! instead. A service injects `Arc<Store>` in its constructor, calls `q()`, and
//! answers `no_db` per request exactly as before; the difference is that the
//! dependency is in the service's own shape rather than in a parameter every
//! caller has to thread through.

use std::sync::Arc;

use crate::db::BlobStore;
use crate::error::V2Error;
use crate::repositories::db::Queries;

/// The plane's data layer, as one injectable handle.
#[derive(Clone, Default)]
pub struct Store {
    pub queries: Option<Arc<Queries>>,
    pub blobs: Option<Arc<BlobStore>>,
    pub hooks: Option<Arc<engine_hooks::HookRunner>>,
}

impl Store {
    pub fn new(
        queries: Option<Arc<Queries>>,
        blobs: Option<Arc<BlobStore>>,
        hooks: Option<Arc<engine_hooks::HookRunner>>,
    ) -> Self {
        Self {
            queries,
            blobs,
            hooks,
        }
    }

    /// The datastore, from the request's DI container when it is registered and
    /// from this handle otherwise. The second half is what keeps services
    /// constructible in a test that installs a state bag but no container.
    pub async fn q(&self) -> Result<Arc<Queries>, V2Error> {
        super::deps::queries(self.queries.clone()).await
    }

    pub async fn blobs(&self) -> Result<Arc<BlobStore>, V2Error> {
        super::deps::blob_store(self.blobs.clone()).await
    }

    pub fn hooks(&self) -> Option<Arc<engine_hooks::HookRunner>> {
        self.hooks.clone()
    }
}
