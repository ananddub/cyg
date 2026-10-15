# auto_ws

`auto_ws` registers [axum](https://crates.io/crates/axum) websockets and their
event handlers at compile time through `inventory`, and resolves every socket
object with [`auto-di`](https://crates.io/crates/auto-di).

## Installation

```toml
[dependencies]
auto-di = "0.6"
auto_ws = "0.1"
axum = { version = "0.8", features = ["ws"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = "1"
```

## DI-managed socket

```rust,no_run
use std::sync::Arc;

use auto_di::singleton;
use auto_ws::{Conn, Msg, WsError, auto_ws};

struct ChatService;

#[singleton]
impl ChatService {
    fn new() -> Self {
        Self
    }
}

struct ChatSocket {
    service: Arc<ChatService>,
}

// `#[auto_ws]` registers the socket with `auto-di` itself, like `auto_route`'s
// `#[controller]`.
#[auto_ws("/ws")]
impl ChatSocket {
    fn new(service: Arc<ChatService>) -> Self {
        Self { service }
    }

    #[connect]
    async fn connect(&self, conn: &Conn) {
        // Build and register the app's per-connection session here.
        let _ = conn.id();
    }

    #[on("message")]
    async fn message(&self, conn: &Conn, msg: &Msg) -> Result<Option<serde_json::Value>, WsError> {
        let _ = (&self.service, conn, msg);
        Ok(Some(serde_json::json!({ "type": "message_ack" })))
    }

    #[disconnect]
    async fn disconnect(&self, conn: &Conn) {
        let _ = conn.id();
    }
}
```

`#[on("tag")]` handlers take `(&self, conn: &Conn, msg: &Msg)` and return
`Result<Option<T>, E>`: `T: Serialize` is written back on the request's `cid`,
`E: Into<WsError>` becomes an error frame. `#[connect]` / `#[disconnect]` hooks
take `(&self, conn: &Conn)` and run in declaration order.

## Start with axum

`mount` needs a protocol: how a request is authenticated, how a frame is
decoded, and how an error is written.

```rust,no_run
use auto_ws::{Identity, Msg, Protocol, WsError};

struct MyProtocol;

impl Protocol for MyProtocol {
    fn authenticate(
        parts: &axum::http::request::Parts,
    ) -> Result<Identity, axum::response::Response> {
        let _ = parts;
        Ok(Identity::new(uuid::Uuid::nil(), "player"))
    }

    fn decode(frame: &str) -> Result<Msg, WsError> {
        let value: serde_json::Value = serde_json::from_str(frame)
            .map_err(|error| WsError::new(3, format!("cannot parse frame: {error}")))?;
        let cid = value.get("cid").and_then(|cid| cid.as_str()).map(str::to_string);
        let event = value.get("type").and_then(|event| event.as_str()).unwrap_or("").to_string();
        Ok(Msg::new(cid, event, value))
    }

    fn encode_error(cid: Option<&str>, error: &WsError) -> String {
        serde_json::json!({ "cid": cid, "type": "error", "code": error.code, "message": error.message })
            .to_string()
    }
}

async fn build(container: &auto_di::Container, router: axum::Router) -> axum::Router {
    auto_ws::mount::<MyProtocol>(container, router).await.expect("sockets resolve")
}
```
