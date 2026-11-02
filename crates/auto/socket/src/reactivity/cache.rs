use std::sync::Arc;

use crate::state::{CACHE_INVALIDATOR, LIVE_ENDPOINT_CACHE};

fn endpoint_cache() -> &'static moka::future::Cache<String, serde_json::Value> {
    LIVE_ENDPOINT_CACHE.get_or_init(|| {
        moka::future::Cache::builder()
            .max_capacity(10_000)
            .time_to_live(std::time::Duration::from_secs(60))
            .support_invalidation_closures()
            .build()
    })
}

fn endpoint_cache_key(
    endpoint: &str,
    args: &serde_json::Value,
    identity: Option<&crate::LiveIdentity>,
) -> String {
    format!(
        "user:{}:{endpoint}:{args}",
        identity.map_or(0, |identity| identity.user_id)
    )
}

pub async fn cache_live_value(
    endpoint: &str,
    args: &serde_json::Value,
    identity: Option<&crate::LiveIdentity>,
    value: serde_json::Value,
) {
    endpoint_cache()
        .insert(endpoint_cache_key(endpoint, args, identity), value)
        .await;
}

pub async fn cached_live_value(
    endpoint: &str,
    args: &serde_json::Value,
    identity: Option<&crate::LiveIdentity>,
) -> Option<serde_json::Value> {
    endpoint_cache()
        .get(&endpoint_cache_key(endpoint, args, identity))
        .await
}

pub fn set_cache_invalidator<F, Fut>(invalidate: F) -> Result<(), &'static str>
where
    F: Fn(Vec<String>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    CACHE_INVALIDATOR
        .set(Arc::new(move |tables| Box::pin(invalidate(tables))))
        .map_err(|_| "live cache invalidator is already configured")
}
