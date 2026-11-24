use auto_di::{BoxFuture, Container, DiError};
use axum::Router;

/// One `#[controller]` impl block, as the plane builds it.
///
/// `versioned` says how its router must be mounted: a versioned controller
/// registered its routes without the `/v1` segment they were authored under, so
/// the plane nests the whole versioned tree under the base it serves. Everything
/// else registers its full path and merges at the root.
pub struct RouteDescriptor {
    pub(crate) factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<Router<()>, DiError>>,
    pub(crate) versioned: bool,
}

impl RouteDescriptor {
    pub const fn new(
        factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<Router<()>, DiError>>,
        versioned: bool,
    ) -> Self {
        Self { factory, versioned }
    }

    /// Whether this controller belongs to the versioned tree.
    pub const fn versioned(&self) -> bool {
        self.versioned
    }
}

inventory::collect!(RouteDescriptor);

/// Every controller, mounted where it was authored: versioned routes keep the
/// `/v1` segment their attributes carry. Prefer [`build_routes_at`] when the
/// plane serves one canonical base with aliases.
pub async fn build_routes(container: &Container) -> Result<Router<()>, DiError> {
    let mut router = Router::new();
    for descriptor in inventory::iter::<RouteDescriptor> {
        router = router.merge((descriptor.factory)(container).await?);
    }
    Ok(router)
}

/// The versioned tree alone, with version segments stripped: the caller mounts
/// it at the canonical base and at any deprecated alias.
pub async fn build_versioned_routes(container: &Container) -> Result<Router<()>, DiError> {
    let mut router = Router::new();
    for descriptor in inventory::iter::<RouteDescriptor> {
        if descriptor.versioned {
            router = router.merge((descriptor.factory)(container).await?);
        }
    }
    Ok(router)
}

/// Everything that is not part of the versioned tree: `/status`, `/metrics`,
/// `/ws/*`, `/dashboard/*`, `/lobby`, `/queue`, `/matchmake`, `/auto/*`.
pub async fn build_root_routes(container: &Container) -> Result<Router<()>, DiError> {
    let mut router = Router::new();
    for descriptor in inventory::iter::<RouteDescriptor> {
        if !descriptor.versioned {
            router = router.merge((descriptor.factory)(container).await?);
        }
    }
    Ok(router)
}

/// Root controllers at the root, the versioned tree nested under `base`.
pub async fn build_routes_at(container: &Container, base: &str) -> Result<Router<()>, DiError> {
    let root = build_root_routes(container).await?;
    let versioned = build_versioned_routes(container).await?;
    Ok(root.nest(base, versioned))
}

pub async fn routes() -> Result<Router<()>, DiError> {
    build_routes(auto_di::global_container()?).await
}
