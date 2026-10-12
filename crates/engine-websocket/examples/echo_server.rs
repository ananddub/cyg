//! Minimal echo server on the WS port from `PORT` (default 8091).
//! Try: node -e 'const w=new WebSocket("ws://127.0.0.1:8091/echo");w.onopen=()=>w.send("hi");w.onmessage=m=>console.log("got:",m.data)'

use std::time::Duration;

use engine_websocket::{WsConfig, WsEvent, WsServer};

fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8091);
    let addr = format!("0.0.0.0:{port}").parse().unwrap();
    let mut server = WsServer::new(addr, WsConfig::default()).expect("bind");
    println!("🔌 WebSocket echo server on ws://{addr}/");

    loop {
        for ev in server.poll_events(Duration::from_millis(100)).unwrap() {
            match ev {
                WsEvent::Connected { client_id, path, .. } => {
                    println!("+ {client_id} ({path}) — ccu {}", server.ccu());
                },
                WsEvent::Text { client_id, text } => {
                    let _ = server.send_text(&client_id, &format!("echo:{text}"));
                },
                WsEvent::Binary { client_id, data } => {
                    let _ = server.send_binary(&client_id, &data);
                },
                WsEvent::Disconnected { client_id, code } => {
                    println!("- {client_id} code {code} — ccu {}", server.ccu());
                },
                WsEvent::Pong { .. } => {},
            }
        }
    }
}
