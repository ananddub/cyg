use auto_di::{BoxFuture, Container, DiError};
pub struct SocketDescriptor {
    pub(crate) namespace: &'static str,
    pub(crate) factory:
        for<'a> fn(&'a Container) -> BoxFuture<'a, Result<SocketRegistrar, DiError>>,
}
pub type SocketRegistrar =
    std::sync::Arc<dyn Fn(socketioxide::extract::SocketRef) + Send + Sync + 'static>;
impl SocketDescriptor {
    pub const fn new(
        namespace: &'static str,
        factory: for<'a> fn(&'a Container) -> BoxFuture<'a, Result<SocketRegistrar, DiError>>,
    ) -> Self {
        Self { namespace, factory }
    }
}

pub struct LiveRefreshDescriptor {
    pub(crate) endpoint: &'static str,
    pub(crate) factory: for<'a> fn(
        &'a Container,
    ) -> BoxFuture<
        'a,
        Result<
            std::sync::Arc<
                dyn Fn(serde_json::Value, Option<crate::LiveIdentity>) -> BoxFuture<'static, ()>
                    + Send
                    + Sync
                    + 'static,
            >,
            DiError,
        >,
    >,
}
impl LiveRefreshDescriptor {
    pub const fn new(
        endpoint: &'static str,
        factory: for<'a> fn(
            &'a Container,
        ) -> BoxFuture<
            'a,
            Result<
                std::sync::Arc<
                    dyn Fn(serde_json::Value, Option<crate::LiveIdentity>) -> BoxFuture<'static, ()>
                        + Send
                        + Sync
                        + 'static,
                >,
                DiError,
            >,
        >,
    ) -> Self {
        Self { endpoint, factory }
    }
}

pub struct LiveAccessDescriptor {
    pub(crate) endpoint: &'static str,
    pub(crate) permission: Option<(&'static str, &'static str)>,
}
impl LiveAccessDescriptor {
    pub const fn authenticated(endpoint: &'static str) -> Self {
        Self {
            endpoint,
            permission: None,
        }
    }
    pub const fn permission(
        endpoint: &'static str,
        resource: &'static str,
        operation: &'static str,
    ) -> Self {
        Self {
            endpoint,
            permission: Some((resource, operation)),
        }
    }
}

pub struct LiveTableDescriptor {
    pub(crate) endpoint: &'static str,
    pub(crate) tables: &'static [&'static str],
}
impl LiveTableDescriptor {
    pub const fn new(endpoint: &'static str, tables: &'static [&'static str]) -> Self {
        Self { endpoint, tables }
    }
}

inventory::collect!(SocketDescriptor);
inventory::collect!(LiveRefreshDescriptor);
inventory::collect!(LiveAccessDescriptor);
inventory::collect!(LiveTableDescriptor);
