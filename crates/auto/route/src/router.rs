use auto_di::{BoxFuture, Container, DiError};
use axum::Router;

pub struct RouteDescriptor {
    pub(crate) factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<Router<()>, DiError>>,
}

impl RouteDescriptor {
    pub const fn new(
        factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<Router<()>, DiError>>,
    ) -> Self {
        Self { factory }
    }
}

inventory::collect!(RouteDescriptor);

pub async fn build_routes(container: &Container) -> Result<Router<()>, DiError> {
    let mut router = Router::new();
    for descriptor in inventory::iter::<RouteDescriptor> {
        router = router.merge((descriptor.factory)(container).await?);
    }
    Ok(router)
}

pub async fn routes() -> Result<Router<()>, DiError> {
    build_routes(auto_di::global_container()?).await
}
