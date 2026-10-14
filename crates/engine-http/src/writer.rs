//! # Response Serialization
//!
//! Turns a protocol-agnostic [`Response`] into wire bytes. Kept separate from
//! the backends so each transport can decorate framing however it needs
//! (HTTP/1.1 status line here; HTTP/3 would reuse the same body/headers).

use std::io::Write;

use crate::types::Response;

/// Serialize a response as an HTTP/1.1 message.
///
/// `keep_alive` controls the `Connection` header. `Content-Length` is always
/// emitted from `body_len`. When `omit_body` is set (HEAD requests) the body
/// bytes are not written, but the declared length still reflects the
/// would-be payload.
///
/// Writes straight into one buffer: no per-header `String` is built.
pub fn serialize_http1(
    response: &Response,
    keep_alive: bool,
    body_len: usize,
    omit_body: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(if omit_body { 128 } else { body_len + 128 });
    serialize_http1_into(&mut out, response, keep_alive, body_len, omit_body);
    out
}

/// [`serialize_http1`] writing into a caller-owned buffer (cleared first), so
/// a server can reuse one allocation for every response it sends.
pub fn serialize_http1_into(
    out: &mut Vec<u8>,
    response: &Response,
    keep_alive: bool,
    body_len: usize,
    omit_body: bool,
) {
    out.clear();

    let _ = write!(
        out,
        "HTTP/1.1 {} {}\r\n",
        response.status,
        Response::status_text(response.status)
    );

    let mut has_server = false;
    let mut has_content_type = false;
    for (name, value) in response.headers.iter() {
        // Content-Length is written explicitly below; skip any duplicate.
        if name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        if name.eq_ignore_ascii_case("server") {
            has_server = true;
        } else if name.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        let _ = write!(out, "{name}: {value}\r\n");
    }

    if !has_server {
        out.extend_from_slice(b"Server: OpenOxide-Engine/0.1\r\n");
    }
    if !has_content_type {
        out.extend_from_slice(b"Content-Type: application/octet-stream\r\n");
    }

    let _ = write!(out, "Content-Length: {body_len}\r\n");
    if !keep_alive {
        out.extend_from_slice(b"Connection: close\r\n");
    }
    out.extend_from_slice(b"\r\n");
    if !omit_body {
        out.extend_from_slice(&response.body);
    }
}
