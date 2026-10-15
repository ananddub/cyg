use auto_di::BoxFuture;
use std::sync::{Arc, OnceLock};

pub(crate) type LiveAuthenticator =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<LiveIdentity, String>> + Send + Sync>;
pub(crate) type LiveAuthorizer = Arc<
    dyn Fn(LiveIdentity, &'static str, &'static str) -> BoxFuture<'static, Result<bool, String>>
        + Send
        + Sync,
>;
pub(crate) static AUTHENTICATOR: OnceLock<LiveAuthenticator> = OnceLock::new();
pub(crate) static AUTHORIZER: OnceLock<LiveAuthorizer> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct LiveIdentity {
    pub user_id: i64,
    pub organization_id: Option<i64>,
}

pub fn set_authorizer<F, Fut>(authorize: F) -> Result<(), &'static str>
where
    F: Fn(LiveIdentity, &'static str, &'static str) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<bool, String>> + Send + 'static,
{
    AUTHORIZER
        .set(Arc::new(move |identity, resource, operation| {
            Box::pin(authorize(identity, resource, operation))
        }))
        .map_err(|_| "live authorizer already configured")
}

pub fn set_authenticator<F, Fut>(authenticate: F) -> Result<(), &'static str>
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<LiveIdentity, String>> + Send + 'static,
{
    AUTHENTICATOR
        .set(Arc::new(move |token| Box::pin(authenticate(token))))
        .map_err(|_| "live authenticator already configured")
}
