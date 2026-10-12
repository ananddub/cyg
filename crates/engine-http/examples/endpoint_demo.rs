//! Demonstrates the transport-agnostic command interface.
//!
//! The SAME endpoint set is served regardless of wire protocol. Pick the
//! protocol at startup with the `HTTP_VERSION` env var (`http1` default):
//!
//! ```bash
//! HTTP_VERSION=http1 cargo run -p engine-http --example endpoint_demo
//! HTTP_VERSION=http3 cargo run -p engine-http --example endpoint_demo  # errors: not yet wired
//! ```
//!
//! Then: `curl -s localhost:8090/status | jq` and
//! `curl -s localhost:8090/rooms/lobby-3`.

use std::net::SocketAddr;

use engine_http::{Api, HttpVersion, Response};

fn main() {
    let version: HttpVersion = std::env::var("HTTP_VERSION")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(HttpVersion::Http1);

    // Endpoints defined ONCE — no protocol knowledge here.
    let api = Api::new()
        .get("/health", |_| Response::text("ok"))
        .get("/status", |req| {
            Response::json(format!(
                r#"{{"peer":"{}","path":"{}"}}"#,
                req.headers.get("x-peer-addr").unwrap_or("n/a"),
                req.path
            ))
        })
        .get("/rooms/:id", |req| {
            let id = req.param("id").unwrap_or("unknown");
            Response::json(format!(r#"{{"room":"{id}","players":0}}"#))
        });

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    println!("Serving {} endpoints over {} on {}", api.endpoint_count(), version.as_str(), addr);

    let handle = match api.serve(version, addr) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("failed to start: {e}");
            std::process::exit(1);
        }
    };
    println!("✓ listening on {}", handle.local_addr());

    // Run until Ctrl+C (a real signal handler is left as an exercise).
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
