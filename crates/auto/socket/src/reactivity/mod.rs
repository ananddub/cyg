use crate::{
    LiveTableDescriptor,
    rooms::active_live_rooms,
    state::{CACHE_INVALIDATOR, LIVE_ENDPOINT_CACHE, LIVE_REFRESHERS, SOCKET_IO},
};

mod cache;
mod subscription;

#[cfg(test)]
mod tests;

pub use cache::{cache_live_value, cached_live_value, set_cache_invalidator};
pub(crate) use subscription::refresh_subscription;

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
    let mut rooms = Vec::new();
    active_live_rooms().iter_sync(|_, room| {
        if affected.contains(room.endpoint.as_str()) {
            rooms.push(room.clone());
        }
        true
    });
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
