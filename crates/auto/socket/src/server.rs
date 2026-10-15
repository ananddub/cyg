use crate::{
    LiveAccessDescriptor, LiveIdentity, SocketDescriptor, SocketRegistrar,
    auth::{AUTHENTICATOR, AUTHORIZER},
    reactivity::refresh_subscription,
    registry::install_refreshers,
    rooms::{LiveSubscriptionRequest, SocketSubscriptions, release, retain, scoped},
    state::{ACCESS_ENDPOINTS, LATEST_CHANNELS, SOCKET_IO, STREAM_HISTORY},
};
use auto_di::{Container, DiError};
use serde::Deserialize;
use socketioxide::{
    SocketIo,
    extract::{Data, SocketRef, TryData},
    socket::DisconnectReason,
};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
struct LiveConnectAuth {
    token: Option<String>,
}
#[derive(Clone, Debug)]
struct SocketIdentity(LiveIdentity);

pub async fn register(io: &SocketIo, container: &Container) -> Result<(), DiError> {
    let _ = SOCKET_IO.set(io.clone());
    let _ = ACCESS_ENDPOINTS.set(
        inventory::iter::<LiveAccessDescriptor>
            .into_iter()
            .map(|item| (item.endpoint, item.permission))
            .collect(),
    );
    install_refreshers(container).await?;
    let mut namespaces: HashMap<&'static str, Vec<SocketRegistrar>> = HashMap::new();
    for descriptor in inventory::iter::<SocketDescriptor> {
        namespaces
            .entry(descriptor.namespace)
            .or_default()
            .push((descriptor.factory)(container).await?);
    }
    for (namespace, registrars) in namespaces {
        register_namespace(io, namespace, registrars);
    }
    Ok(())
}
fn register_namespace(io: &SocketIo, namespace: &'static str, registrars: Vec<SocketRegistrar>) {
    io.ns(
        namespace,
        move |socket: SocketRef, TryData(auth): TryData<LiveConnectAuth>| {
            let registrars = registrars.clone();
            async move {
                if !authenticate(&socket, auth.ok()).await {
                    return;
                }
                bind_subscribe(&socket, namespace);
                bind_unsubscribe(&socket);
                bind_disconnect(&socket);
                for registrar in registrars {
                    registrar(socket.clone());
                }
                // Namespace handlers are installed asynchronously after the
                // Socket.IO `connect` packet.  Let clients safely wait until
                // their events are bound instead of losing the first emit.
                let _ = socket.emit("socket:ready", &serde_json::json!({"namespace": namespace}));
            }
        },
    );
}
async fn authenticate(socket: &SocketRef, auth: Option<LiveConnectAuth>) -> bool {
    let Some(authenticator) = AUTHENTICATOR.get() else {
        return true;
    };
    let Some(token) = auth.and_then(|auth| auth.token) else {
        let _ = socket.clone().disconnect();
        return false;
    };
    match authenticator(token).await {
        Ok(identity) => {
            socket.extensions.insert(SocketIdentity(identity));
            true
        }
        Err(_) => {
            let _ = socket.clone().disconnect();
            false
        }
    }
}
fn bind_subscribe(socket: &SocketRef, namespace: &'static str) {
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
            // A subscription can be created after a server restart, when the
            // server-side live cache/room history is empty. Always request one
            // authorized client hydration so every topic starts in sync.
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
    let latest = LATEST_CHANNELS
        .get_or_init(Default::default)
        .lock()
        .ok()
        .and_then(|items| items.get(&key).map(|sender| sender.borrow().clone()));
    if let Some(latest) = latest {
        let _ = socket.emit("live:update", &latest);
    }
    if let Ok(histories) = STREAM_HISTORY.get_or_init(Default::default).lock() {
        if let Some(history) = histories.get(&key) {
            for event in &history.events {
                let _ = socket.emit("live:update", event);
            }
        }
    }
}
fn bind_unsubscribe(socket: &SocketRef) {
    socket.on(
        "live:unsubscribe",
        |socket: SocketRef, Data(request): Data<LiveSubscriptionRequest>| async move {
            let auth = ACCESS_ENDPOINTS
                .get()
                .is_some_and(|items| items.contains_key(request.endpoint.as_str()));
            let identity = auth
                .then(|| {
                    socket
                        .extensions
                        .get::<SocketIdentity>()
                        .map(|value| value.0)
                })
                .flatten();
            let room = scoped(identity.as_ref(), &request.endpoint, &request.args);
            socket.leave(room.clone());
            let mut subscriptions = socket
                .extensions
                .get::<SocketSubscriptions>()
                .map(|value| value.0)
                .unwrap_or_default();
            let removed = subscriptions.remove(&room);
            socket.extensions.insert(SocketSubscriptions(subscriptions));
            if removed {
                release(&room);
            }
        },
    );
}
fn bind_disconnect(socket: &SocketRef) {
    socket.on_disconnect(|socket: SocketRef, reason: DisconnectReason| async move {
        tracing::info!(socket_id = %socket.id, reason = ?reason, "live socket disconnected");
        if let Some(items) = socket.extensions.get::<SocketSubscriptions>() {
            for room in items.0 {
                release(&room);
            }
        }
    });
}
pub async fn register_global(io: &SocketIo) -> Result<(), DiError> {
    register(io, auto_di::global_container()?).await
}
