use crate::{
    LiveTableDescriptor,
    rooms::ACTIVE_LIVE_ROOMS,
    state::{
        CACHE_INVALIDATOR, LIVE_ENDPOINT_CACHE, LIVE_REFRESHERS, SOCKET_IO,
        SUBSCRIPTION_REFRESH_GATES,
    },
};
use std::sync::Arc;

pub fn notify_table_changes<'a>(tables: impl IntoIterator<Item = &'a str>) {
    let changed = tables.into_iter().collect::<std::collections::HashSet<_>>();
    tracing::info!(tables = ?changed, "live table notification received");
    refresh_or_invalidate_rooms(&changed);
}

fn refresh_or_invalidate_rooms(changed: &std::collections::HashSet<&str>) {
    let affected = inventory::iter::<LiveTableDescriptor>
        .into_iter()
        .filter(|entry| entry.tables.iter().any(|table| changed.contains(table)))
        .map(|entry| entry.endpoint)
        .collect::<std::collections::HashSet<_>>();
    if affected.is_empty() {
        tracing::info!("live table notification has no matching endpoint");
        return;
    }
    let rooms = ACTIVE_LIVE_ROOMS
        .get_or_init(Default::default)
        .lock()
        .map(|rooms| {
            rooms
                .values()
                .filter(|room| affected.contains(room.endpoint.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tracing::info!(endpoints = ?affected, rooms = rooms.len(), "live rooms selected for refresh");
    let changed_tables = changed
        .iter()
        .map(|table| (*table).to_owned())
        .collect::<Vec<_>>();
    tokio::spawn(async move {
        if let Some(cache) = LIVE_ENDPOINT_CACHE.get() {
            for endpoint in &affected {
                let needle = format!(":{endpoint}:");
                let _ = cache.invalidate_entries_if(move |key, _| key.contains(&needle));
            }
            cache.run_pending_tasks().await;
        }
        if let Some(invalidate) = CACHE_INVALIDATOR.get() {
            invalidate(changed_tables).await;
        }
        tracing::info!(
            rooms = rooms.len(),
            "live cache invalidation completed before refresh"
        );
        let Some(io) = SOCKET_IO.get() else {
            return;
        };
        let mut broadcast_endpoints = std::collections::HashSet::new();
        for room in rooms {
            if let Some(refresher) = LIVE_REFRESHERS
                .get()
                .and_then(|items| items.iter().find(|item| item.endpoint == room.endpoint))
            {
                tracing::info!(endpoint = room.endpoint, room = %room.room, args = ?room.args, "refreshing live room");
                (refresher.refresh)(room.args.clone(), room.identity.clone()).await;
                tracing::info!(endpoint = room.endpoint, room = %room.room, "live room refresh completed");
                continue;
            }
            if let Some(namespace) = io.of(room.namespace) {
                if !claim_endpoint_invalidation(
                    &mut broadcast_endpoints,
                    room.namespace,
                    &room.endpoint,
                ) {
                    continue;
                }
                tracing::info!(endpoint = room.endpoint, room = %room.room, args = ?room.args, "no server refresher; broadcasting live invalidation");
                match namespace
                    .emit("live:invalidate", &endpoint_invalidation(&room.endpoint))
                    .await
                {
                    Ok(()) => tracing::info!(
                        endpoint = room.endpoint,
                        "live invalidation broadcast to namespace"
                    ),
                    Err(error) => {
                        tracing::warn!(endpoint = room.endpoint, error = %error, "live invalidation send failed")
                    }
                }
            } else {
                tracing::warn!(
                    endpoint = room.endpoint,
                    namespace = room.namespace,
                    "cannot send live invalidation: namespace missing"
                );
            }
        }
    });
}

fn claim_endpoint_invalidation(
    claimed: &mut std::collections::HashSet<(&'static str, String)>,
    namespace: &'static str,
    endpoint: &str,
) -> bool {
    claimed.insert((namespace, endpoint.to_owned()))
}

fn endpoint_invalidation(endpoint: &str) -> serde_json::Value {
    serde_json::json!({"endpoint": endpoint, "args": null})
}

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

pub(crate) async fn refresh_subscription(
    endpoint: &str,
    args: serde_json::Value,
    identity: Option<crate::LiveIdentity>,
) {
    let Some(refreshers) = LIVE_REFRESHERS.get() else {
        return;
    };
    for entry in refreshers.iter().filter(|entry| entry.endpoint == endpoint) {
        let gate = {
            let mut gates = SUBSCRIPTION_REFRESH_GATES
                .get_or_init(Default::default)
                .lock()
                .expect("subscription refresh registry poisoned");
            gates
                .entry(endpoint.to_owned())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = gate.lock().await;
        (entry.refresh)(args.clone(), identity.clone()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_endpoint_invalidation, endpoint_invalidation};
    use std::collections::HashSet;

    #[test]
    fn coalesces_query_variants_into_one_endpoint_invalidation() {
        let mut claimed = HashSet::new();

        assert!(claim_endpoint_invalidation(
            &mut claimed,
            "/_openoxide/live",
            "DeploymentController::list",
        ));
        assert!(!claim_endpoint_invalidation(
            &mut claimed,
            "/_openoxide/live",
            "DeploymentController::list",
        ));
        assert!(claim_endpoint_invalidation(
            &mut claimed,
            "/_openoxide/live",
            "ComposeController::get",
        ));
        assert!(claim_endpoint_invalidation(
            &mut claimed,
            "/another-namespace",
            "DeploymentController::list",
        ));
    }

    #[test]
    fn endpoint_invalidation_explicitly_targets_every_args_variant() {
        assert_eq!(
            endpoint_invalidation("DeploymentController::list"),
            serde_json::json!({
                "endpoint": "DeploymentController::list",
                "args": null,
            }),
        );
    }
}
