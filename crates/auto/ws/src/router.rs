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

/// What an `#[on]` handler returns: `Some` is written back on the request's
/// `cid`, `None` means the handler answered out of band (or not at all).
pub type Reply = Result<Option<Value>, WsError>;

/// Turn a handler's own result into a [`Reply`]: the body is serialised, the
/// error becomes a [`WsError`].
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

/// Resolve every `#[auto_ws]` declaration through `container` and add the
/// sockets to `router`.
///
/// One route per declared path; a path declared by several impls serves all
/// their handlers. Failing to resolve a socket is an error so the caller can
/// decide what a broken DI graph means for its surface.
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
            get(
                move |ws: WebSocketUpgrade, parts: Parts| {
                    let registrar = Arc::clone(&registrar);
                    async move {
                        match P::authenticate(&parts) {
                            Ok(identity) => ws.on_upgrade(move |socket| {
                                serve::<P>(socket, identity, registrar)
                            }),
                            Err(response) => response,
                        }
                    }
                },
            ),
        );
    }
    Ok(router)
}

/// One connection: hooks, read loop, teardown.
///
/// The writer is the connection's own channel, so replies a handler queues and
/// frames another plane pushes take the same ordered path. Protocol pings are
/// answered here; binary frames are not part of the protocol and are skipped.
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
                // Every writer is gone: nothing can reach this socket anymore.
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
                // Binary frames: not part of a text protocol.
                Some(Ok(_)) => continue,
            }
        }
    }

    for hook in &registrar.disconnects {
        hook(conn.clone()).await;
    }
}

async fn dispatch<P: Protocol>(conn: &Conn, registrar: &SocketRegistrar, msg: Msg) {
    // An unhandled tag is a message the client should not have sent (in this
    // protocol: a server-pushed type); it is ignored, not answered.
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
