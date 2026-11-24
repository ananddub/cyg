use std::collections::HashMap;
use std::sync::Arc;

use auto_di::{Container, DiError};
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::request::Parts;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{Conn, Msg, Protocol, SocketDescriptor, SocketRegistrar, WsError};

pub type Reply = Result<Option<Value>, WsError>;

pub fn reply<T, E>(out: Result<Option<T>, E>) -> Reply
where
    T: Serialize,
    E: Into<WsError>,
{
    match out {
        Ok(Some(body)) => serde_json::to_value(body)
            .map(Some)
            .map_err(|error| WsError::new(format!("cannot serialise reply: {error}"))),
        Ok(None) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub async fn mount<P: Protocol>(container: &Container, router: Router) -> Result<Router, DiError> {
    let mut sockets: HashMap<&'static str, SocketRegistrar> = HashMap::new();
    for descriptor in inventory::iter::<SocketDescriptor> {
        let registrar = (descriptor.factory)(container).await?;
        sockets.entry(descriptor.path).or_default().merge(registrar);
    }

    let mut router = router;
    for (path, registrar) in sockets {
        let registrar = Arc::new(registrar);
        router = router.route(
            path,
            get(move |ws: WebSocketUpgrade, parts: Parts| {
                let registrar = Arc::clone(&registrar);
                async move {
                    match P::authenticate(&parts) {
                        Ok(identity) => {
                            ws.on_upgrade(move |socket| serve::<P>(socket, identity, registrar))
                        }
                        Err(response) => response,
                    }
                }
            }),
        );
    }
    Ok(router)
}

async fn serve<P: Protocol>(
    socket: WebSocket,
    identity: crate::Identity,
    registrar: Arc<SocketRegistrar>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let conn = Conn::new(Uuid::new_v4(), identity, tx);

    for hook in &registrar.connects {
        hook(conn.clone()).await;
    }

    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            outbound = rx.recv() => match outbound {
                Some(line) => {
                    if sink.send(Message::Text(line.into())).await.is_err() {
                        break;
                    }
                }

                None => break,
            },
            inbound = stream.next() => match inbound {
                Some(Ok(Message::Text(text))) => match P::decode(&text) {
                    Ok(msg) => dispatch::<P>(&conn, &registrar, msg).await,
                    Err(error) => conn.send_line(P::encode_error(None, &error)),
                },
                Some(Ok(Message::Ping(payload))) => {
                    if sink.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,

                Some(Ok(_)) => continue,
            }
        }
    }

    for hook in &registrar.disconnects {
        hook(conn.clone()).await;
    }
}

async fn dispatch<P: Protocol>(conn: &Conn, registrar: &SocketRegistrar, msg: Msg) {
    let Some(handler) = registrar.events.get(msg.event()) else {
        return;
    };
    let cid = msg.cid().map(str::to_string);
    match handler(conn.clone(), msg).await {
        Ok(Some(body)) => conn.reply(cid.as_deref(), &body),
        Ok(None) => {}
        Err(error) => conn.send_line(P::encode_error(cid.as_deref(), &error)),
    }
}
