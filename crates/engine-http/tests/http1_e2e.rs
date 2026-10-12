//! End-to-end tests for the HTTP/1.1 backend over a real socket.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::Duration;

use engine_http::{Api, HttpVersion, Response};

fn free_addr() -> std::net::SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn read_full(stream: &mut TcpStream) -> String {
    // Responses sent with `Connection: close` terminate with EOF.
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok();
    String::from_utf8_lossy(&buf).to_string()
}

fn sample_api() -> Api {
    Api::new()
        .get("/health", |_| Response::text("ok"))
        .get("/rooms/:id", |req| {
            Response::json(format!(
                r#"{{"room":"{}"}}"#,
                req.param("id").unwrap_or("?")
            ))
        })
        .post("/echo", |req| {
            let body = req.body_str().unwrap_or("").to_string();
            Response::json(format!(r#"{{"echo":"{}"}}"#, body.replace('"', "'")))
        })
}

#[test]
fn get_health_over_socket() {
    let handle = sample_api().serve(HttpVersion::Http1, free_addr()).unwrap();
    let addr = handle.local_addr();

    let mut sock = TcpStream::connect(addr).unwrap();
    sock.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let resp = read_full(&mut sock);
    assert!(resp.contains("200 OK"), "got: {resp}");
    assert!(resp.ends_with("ok"), "got: {resp}");

    handle.shutdown();
}

#[test]
fn path_param_route() {
    let handle = sample_api().serve(HttpVersion::Http1, free_addr()).unwrap();
    let addr = handle.local_addr();

    let mut sock = TcpStream::connect(addr).unwrap();
    sock.write_all(b"GET /rooms/42 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let resp = read_full(&mut sock);
    assert!(resp.contains("200 OK"), "got: {resp}");
    assert!(resp.contains(r#""room":"42""#), "got: {resp}");

    handle.shutdown();
}

#[test]
fn post_with_content_length_body() {
    let handle = sample_api().serve(HttpVersion::Http1, free_addr()).unwrap();
    let addr = handle.local_addr();

    let body = "hello-api";
    let req = format!(
        "POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.write_all(req.as_bytes()).unwrap();
    let resp = read_full(&mut sock);
    assert!(resp.contains("200 OK"), "got: {resp}");
    assert!(resp.contains(r#""echo":"hello-api""#), "got: {resp}");

    handle.shutdown();
}

#[test]
fn unknown_route_returns_404() {
    let handle = sample_api().serve(HttpVersion::Http1, free_addr()).unwrap();
    let addr = handle.local_addr();

    let mut sock = TcpStream::connect(addr).unwrap();
    sock.write_all(b"GET /nope HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let resp = read_full(&mut sock);
    assert!(resp.contains("404"), "got: {resp}");

    handle.shutdown();
}

#[test]
fn keep_alive_two_requests_one_connection() {
    let handle = sample_api().serve(HttpVersion::Http1, free_addr()).unwrap();
    let addr = handle.local_addr();

    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    sock.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();

    let mut total = Vec::new();
    let mut chunk = [0u8; 1024];
    // Read first response (headers + body ~ up to the blank line + 'ok').
    while !total.windows(5).any(|w| w == b"\r\nok\r\n") && !total.ends_with(b"ok") {
        let n = sock.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        total.extend_from_slice(&chunk[..n]);
    }
    let first = String::from_utf8_lossy(&total).to_string();
    assert!(first.contains("200 OK"), "got: {first}");

    // Second request on the same connection.
    total.clear();
    sock.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let second = read_full(&mut sock);
    assert!(second.contains("200 OK"), "got: {second}");

    let _ = sock.shutdown(Shutdown::Both);
    let served = handle.requests_served();
    handle.shutdown();
    assert!(served >= 2);
}
