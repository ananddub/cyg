# auto_socket

`auto_socket` automatically registers [Socketioxide](https://crates.io/crates/socketioxide) events and resolves handler objects with [`auto-di`](https://crates.io/crates/auto-di).

## Installation

```toml
[dependencies]
auto-di = "0.6"
auto_socket = "0.1"
axum = "0.8"
socketioxide = "0.18"
tokio = { version = "1", features = ["full"] }
```

## DI-managed socket handler

```rust,no_run
use auto_di::singleton;
use auto_socket::auto_socket;
use socketioxide::extract::{Data, SocketRef};

struct ChatSocket;

#[singleton]
#[auto_socket("/chat")]
impl ChatSocket {
    fn new() -> Self {
        Self
    }

    #[on("message")]
    async fn message(&self, socket: SocketRef, Data(message): Data<String>) {
        let _ = socket.emit("message", &message);
    }
}
```

## Standalone event

The namespace is optional and defaults to `/`:

```rust,no_run
use auto_socket::on;
use socketioxide::extract::SocketRef;

#[on("ping")]
async fn ping(socket: SocketRef) {
    let _ = socket.emit("pong", &"ok");
}

#[on("message", namespace = "/chat")]
async fn chat_message(_socket: SocketRef) {}
```

## Module event group

```rust,no_run
use auto_socket::auto_socket;

#[auto_socket("/room")]
mod room_events {
    use socketioxide::extract::{Data, SocketRef};

    #[on("join")]
    async fn join(_socket: SocketRef, Data(_room): Data<String>) {}
}
```

Only inline modules are supported.

## Start with Axum

Call `register_global` after creating `SocketIo` and before serving the application:

```rust,no_run
use axum::Router;
use socketioxide::SocketIo;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let (layer, io) = SocketIo::new_layer();
    auto_socket::register_global(&io)
        .await
        .expect("failed to register socket handlers");

    let app = Router::new().layer(layer);
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Registrations sharing a namespace are grouped so Socketioxide installs each namespace once. Handler objects are singleton values resolved by `auto-di`.

## Typed live publishing

An `#[auto_socket]` group can mix traditional inbound events with typed live outputs. `#[live]`
uses the method name as its event; `#[on("event:name")]` uses the developer's exact event name.
The macro distinguishes inbound handlers by their `SocketRef`/`Data<T>` arguments.

```rust,ignore
#[auto_socket("/servers")]
impl ServerSocket {
    #[live]
    async fn metrics(&self, server_id: i64) -> Metrics {
        self.service.metrics(server_id).await
    }

    #[on("alerts:update")]
    async fn alerts(&self, server_id: i64) -> Vec<Alert> {
        self.service.alerts(server_id).await
    }

    #[on("input")]
    async fn input(&self, socket: SocketRef, Data(input): Data<Input>) {
        // Existing inbound event handling remains unchanged.
    }
}

server_live::metrics_event()
    .emit(&socket, metrics)?;

server_live::alerts_event()
    .broadcast(alerts)
    .await?;

server_live::metrics(server_id)
    .publish(metrics)
    .await?;

// Generated typed client metadata (usable by a Rust/WASM client or a code generator).
let subscription = server_live::metrics_subscription(server_id)?;
transport.emit("live:subscribe", subscription.message())?;
transport.on(subscription.event(), move |json| {
    let metrics: Metrics = subscription.decode(json)?;
    // update client state
});
```

The `#[auto_socket]` namespace is the group. The method return type determines the payload accepted
by both operations. `emit` requires an explicit `SocketRef` and targets only that client;
`broadcast` requires no socket and sends the payload to every client connected to the namespace;
`publish` takes the endpoint arguments in the generated function and targets their matching room.
The generated `_event()` handle is parameter-independent and is used for direct `emit` and
namespace-wide `broadcast` delivery.

## License

MIT
