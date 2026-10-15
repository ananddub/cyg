# auto_route

`auto_route` adds Spring-style route attributes to Axum and integrates controller construction with [`auto-di`](https://crates.io/crates/auto-di).

You write `#[controller]`, `#[get]`, and `#[post]`; the crate generates ordinary Axum routes and discovers them automatically at startup.
`#[controller]` also registers its impl as an auto-di singleton, so it must contain a `new(...) -> Self` constructor and does not need a separate `#[singleton]` attribute.

## Installation

```toml
[dependencies]
auto-di = "0.6"
auto_route = "0.2"
axum = "0.8"
tokio = { version = "1", features = ["full"] }
```

## DI-managed controller

```rust,no_run
use std::sync::Arc;

use auto_di::singleton;
use auto_route::controller;
use axum::{extract::Path, response::IntoResponse};

struct UserService;

#[singleton]
impl UserService {
    fn new() -> Self {
        Self
    }
}

struct UserController {
    service: Arc<UserService>,
}

#[controller("/users")]
impl UserController {
    fn new(service: Arc<UserService>) -> Self {
        Self { service }
    }

    #[get("/:id")]
    async fn get_user(&self, Path(id): Path<u64>) -> impl IntoResponse {
        let _ = &self.service;
        format!("User {id}")
    }

    #[post]
    async fn create_user(&self) -> impl IntoResponse {
        "created"
    }
}
```

`/:id` is accepted for convenience and converted to Axum 0.8's `/{id}` syntax.

## Standalone route

No controller is required for a normal function:

```rust,no_run
use auto_route::get;

#[get("/health")]
async fn health() -> &'static str {
    "ok"
}
```

## Module route group

An inline module can provide a shared base path without a controller object:

```rust,no_run
use auto_route::controller;

#[controller("/admin")]
mod admin_routes {
    #[get("/health")]
    async fn health() -> &'static str {
        "ok"
    }
}
```

Only inline modules are supported. `#[controller] mod routes;` cannot be inspected by a procedural macro.

## Start the server

Call `auto_route::routes()` once while constructing the application:

```rust,no_run
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = auto_route::routes().await.expect("failed to build routes");
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Every controller and standalone route submits a small factory to a compile-time `inventory` registry. At startup, `auto_route` resolves controller singletons through `auto-di`, builds their Axum routers, and merges them into one application router. Macros do not run for each request.

Supported method attributes: `get`, `post`, `put`, `delete`, `patch`, `options`, and `head`.

## Typed live routes

A controller route can also become a typed live endpoint:

```rust,ignore
#[get("")]
#[live("todos", table = "todos")]
async fn list(&self) -> Json<Vec<Todo>> {
    Json(self.repository.list().await)
}
```

The alias `todos` generates controller-centric handles:

```rust,ignore
TodoController::todos()?.publish(todos).await?;
let subscription = TodoController::todos_subscription()?;
```

For results depending on several SQLite tables:

```rust,ignore
#[live("dashboard", tables = ["users", "projects", "deployments"])]
```

Committed writes to any declared table automatically execute a zero-argument `Json<T>` live route once and publish its result to all subscribers. Mutation routes only perform their database write; they do not manually refresh or publish the live query.

Automatic table-backed refresh currently requires a zero-argument controller method returning `Json<T>`. Parameterized live endpoints retain typed explicit publishing.

Delivery can be selected explicitly:

```rust,ignore
#[live("metrics", strategy = latest)]
#[live("logs", strategy = stream, capacity = 512, replay = 50)]
#[live("status", strategy = publish)]
```

`latest` coalesces queued state updates. `stream` preserves order through a bounded queue, applies async backpressure, and can replay a bounded in-memory history to newly joined sockets. A table declaration defaults to `sqlite`; an endpoint without tables defaults to `publish`.

See [Typed realtime endpoints](../../../docs/realtime.md) for setup, runtime behavior, React hook generation, deduplication, and constraints.

## License

MIT
