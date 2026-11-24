use super::SocketIdentity;
use crate::{
    auth::AUTHENTICATOR, rooms::SocketSubscriptions, rooms::release, state::ACCESS_ENDPOINTS,
};
use socketioxide::{
    extract::{Data, SocketRef},
    socket::DisconnectReason,
};

use super::LiveConnectAuth;

pub(super) async fn authenticate(socket: &SocketRef, auth: Option<LiveConnectAuth>) -> bool {
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

pub(super) fn bind_unsubscribe(socket: &SocketRef) {
    socket.on(
        "live:unsubscribe",
        |socket: SocketRef, Data(request): Data<crate::rooms::LiveSubscriptionRequest>| async move {
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
            let room = crate::rooms::scoped(identity.as_ref(), &request.endpoint, &request.args);
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

pub(super) fn bind_disconnect(socket: &SocketRef) {
    socket.on_disconnect(|socket: SocketRef, reason: DisconnectReason| async move {
        tracing::info!(socket_id = %socket.id, reason = ?reason, "live socket disconnected");
        if let Some(items) = socket.extensions.get::<SocketSubscriptions>() {
            for room in items.0 {
                release(&room);
            }
        }
    });
}
