use super::SocketIdentity;
use crate::{
    LiveIdentity, auth::AUTHORIZER, reactivity::refresh_subscription,
    rooms::{LiveSubscriptionRequest, SocketSubscriptions, retain, scoped},
    state::{ACCESS_ENDPOINTS, latest_channels, stream_history},
};
use socketioxide::extract::{Data, SocketRef};

pub(super) fn bind_subscribe(socket: &SocketRef, namespace: &'static str) {
    socket.on(
        "live:subscribe",
        move |socket: SocketRef, Data(request): Data<LiveSubscriptionRequest>| async move {
            let access = ACCESS_ENDPOINTS
                .get()
                .and_then(|items| items.get(request.endpoint.as_str()))
                .copied();
            let identity = access
                .is_some()
                .then(|| {
                    socket
                        .extensions
                        .get::<SocketIdentity>()
                        .map(|value| value.0)
                })
                .flatten();
            let Some(identity) = identity.or_else(|| {
                access.is_none().then(|| LiveIdentity {
                    user_id: 0,
                    organization_id: None,
                })
            }) else {
                return;
            };
            if let Some(Some((resource, operation))) = access {
                let Some(authorizer) = AUTHORIZER.get() else {
                    return;
                };
                if !matches!(
                    authorizer(identity.clone(), resource, operation).await,
                    Ok(true)
                ) {
                    return;
                }
            }
            let room = scoped(
                access.is_some().then_some(&identity),
                &request.endpoint,
                &request.args,
            );
            tracing::info!(socket_id = %socket.id, endpoint = %request.endpoint, args = ?request.args, room = %room, "live subscription accepted");
            socket.join(room.clone());
            let mut subscriptions = socket
                .extensions
                .get::<SocketSubscriptions>()
                .map(|value| value.0)
                .unwrap_or_default();
            let first = subscriptions.insert(room.clone());
            socket.extensions.insert(SocketSubscriptions(subscriptions));
            if first {
                retain(
                    namespace,
                    &room,
                    &request,
                    access.is_some().then_some(identity.clone()),
                );
            }
            refresh_subscription(
                &request.endpoint,
                request.args.clone(),
                access.is_some().then_some(identity),
            )
            .await;
            replay(&socket, namespace, &room);
            let _ = socket.emit(
                "live:subscribed",
                &serde_json::json!({"endpoint": request.endpoint, "args": request.args}),
            );

            let _ = socket.emit(
                "live:invalidate",
                &serde_json::json!({"endpoint": request.endpoint, "args": request.args}),
            );
            tracing::info!(socket_id = %socket.id, endpoint = %request.endpoint, room = %room, "live subscription acknowledged");
        },
    );
}

fn replay(socket: &SocketRef, namespace: &str, room: &str) {
    let key = format!("{namespace}:{room}");
    let latest = latest_channels().read_sync(&key, |_, sender| sender.borrow().clone());
    if let Some(latest) = latest {
        let _ = socket.emit("live:update", &latest);
    }
    stream_history().read_sync(&key, |_, history| {
        for event in &history.events {
            let _ = socket.emit("live:update", event);
        }
    });
}
