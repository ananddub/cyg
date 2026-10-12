//! # Response Serialization
//!
//! Turns a protocol-agnostic [`Response`] into wire bytes. Kept separate from
//! the backends so each transport can decorate framing however it needs
//! (HTTP/1.1 status line here; HTTP/3 would reuse the same body/headers).

use crate::types::Response;

/// Serialize a response as an HTTP/1.1 message.
///
/// `keep_alive` controls the `Connection` header. `Content-Length` is always
/// emitted from `body_len`. When `omit_body` is set (HEAD requests) the body
/// bytes are not written, but the declared length still reflects the
/// would-be payload.
pub fn serialize_http1(
    response: &Response,
    keep_alive: bool,
    body_len: usize,
    omit_body: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(if omit_body { 128 } else { body_len + 128 });

    out.extend_from_slice(
        format!(
            "HTTP/1.1 {} {}\r\n",
            response.status,
            Response::status_text(response.status)
        )
        .as_bytes(),
    );

    let mut has_server = false;
    let mut has_content_type = false;
    for (name, value) in response.headers.iter() {
        let lower = name.to_ascii_lowercase();
        if lower == "server" {
            has_server = true;
        }
        if lower == "content-type" {
            has_content_type = true;
        }
        // Content-Length is written explicitly below; skip any duplicate.
        if lower == "content-length" {
            continue;
        }
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }

    if !has_server {
        out.extend_from_slice(b"Server: OpenOxide-Engine/0.1\r\n");
    }
    if !has_content_type {
        out.extend_from_slice(b"Content-Type: application/octet-stream\r\n");
    }

    out.extend_from_slice(format!("Content-Length: {body_len}\r\n").as_bytes());
    if !keep_alive {
        out.extend_from_slice(b"Connection: close\r\n");
    }
    out.extend_from_slice(b"\r\n");
    if !omit_body {
        out.extend_from_slice(&response.body);
    }
    out
}
