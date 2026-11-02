use crate::{
    LiveAccessDescriptor, LiveIdentity, SocketDescriptor, SocketRegistrar,
    registry::install_refreshers,
    state::{ACCESS_ENDPOINTS, SOCKET_IO},
};
use auto_di::{Container, DiError};
use serde::Deserialize;
use socketioxide::{
    SocketIo,
    extract::{SocketRef, TryData},
};
use std::collections::HashMap;

mod handlers;
mod subscribe;

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

pub async fn register_global(io: &SocketIo) -> Result<(), DiError> {
    register(io, auto_di::global_container()?).await
}

fn register_namespace(io: &SocketIo, namespace: &'static str, registrars: Vec<SocketRegistrar>) {
    io.ns(
        namespace,
        move |socket: SocketRef, TryData(auth): TryData<LiveConnectAuth>| {
            let registrars = registrars.clone();
            async move {
                if !handlers::authenticate(&socket, auth.ok()).await {
                    return;
                }
                subscribe::bind_subscribe(&socket, namespace);
                handlers::bind_unsubscribe(&socket);
                handlers::bind_disconnect(&socket);
                for registrar in registrars {
                    registrar(socket.clone());
                }

                let _ = socket.emit("socket:ready", &serde_json::json!({"namespace": namespace}));
            }
        },
    );
}
