//! # Opening Handshake (RFC 6455 §4.1)
//!
//! Parses the HTTP/1.1 `Upgrade: websocket` request using the engine-http
//! request parser, validates the required headers, and produces the 101
//! response with `Sec-WebSocket-Accept`.

use engine_http::Method;
use engine_http::parse::ParseOutcome;
use engine_http::{Headers, Request, RequestDecoder};

use crate::base64::base64_encode;
use crate::sha1::sha1;

pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Compute `Sec-WebSocket-Accept` from a client `Sec-WebSocket-Key`.
pub fn accept_key(client_key: &str) -> String {
    let digest = sha1(format!("{client_key}{WS_GUID}").as_bytes());
    base64_encode(&digest)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    /// Not a valid HTTP/1.1 request line/headers.
    Malformed(&'static str),
    /// Missing `Upgrade: websocket` or `Connection: upgrade`.
    NotWebSocket,
    /// Missing `Sec-WebSocket-Key`.
    MissingKey,
    /// `Sec-WebSocket-Version` absent or not 13.
    UnsupportedVersion(u16),
    /// Method is not GET.
    NotGet,
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Malformed(s) => s,
            Self::NotWebSocket => "upgrade to websocket required",
            Self::MissingKey => "missing sec-websocket-key",
            Self::UnsupportedVersion(v) => return write!(f, "unsupported ws version {v}"),
            Self::NotGet => "websocket handshake requires GET",
        };
        f.write_str(msg)
    }
}

/// A parsed handshake request: `path`, `query`, and the negotiated key.
pub struct HandshakeRequest {
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Headers,
    pub key: String,
}

/// Validate a handshake request and extract the `Sec-WebSocket-Key`.
pub fn validate(request: &Request) -> Result<HandshakeRequest, HandshakeError> {
    if request.method != Method::Get {
        return Err(HandshakeError::NotGet);
    }

    let header_has = |name: &str, want: &str| -> bool {
        request
            .headers
            .get(name)
            .map(|v| v.split(',').any(|p| p.trim().eq_ignore_ascii_case(want)))
            .unwrap_or(false)
    };

    if !header_has("sec-websocket-version", "13") {
        let v = request
            .headers
            .get("sec-websocket-version")
            .and_then(|s| s.trim().parse::<u16>().ok())
            .unwrap_or(0);
        return Err(HandshakeError::UnsupportedVersion(v));
    }
    if !header_has("upgrade", "websocket") || !header_has("connection", "upgrade") {
        return Err(HandshakeError::NotWebSocket);
    }

    let key = request
        .headers
        .get("sec-websocket-key")
        .map(|s| s.trim().to_string())
        .ok_or(HandshakeError::MissingKey)?;

    Ok(HandshakeRequest {
        path: request.path.clone(),
        query: request
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        headers: request.headers.clone(),
        key,
    })
}

/// Build the `101 Switching Protocols` response bytes.
pub fn accept_response(req: &HandshakeRequest, extra_headers: &[(&str, String)]) -> Vec<u8> {
    let accept = accept_key(&req.key);
    let mut out = String::with_capacity(256);
    out.push_str("HTTP/1.1 101 Switching Protocols\r\n");
    out.push_str("Upgrade: websocket\r\n");
    out.push_str("Connection: Upgrade\r\n");
    out.push_str(&format!("Sec-WebSocket-Accept: {accept}\r\n"));
    for (name, value) in extra_headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// Plain HTTP error for rejected upgrades (before framing kicks in).
pub fn reject_response(status: u16, message: &str) -> Vec<u8> {
    let body = format!(r#"{{"error":"{message}"}}"#);
    format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        reason_phrase(status) as &str,
        body.len()
    )
    .into_bytes()
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        405 => "Method Not Allowed",
        _ => "Upgrade Required",
    }
}

/// Result of feeding handshake bytes into a connecting socket.
pub enum HandshakeOutcome {
    /// Still buffering the request head.
    NeedMore,
    /// Valid upgrade; contains the parsed request.
    Ok(HandshakeRequest),
    /// Reject with this HTTP status + reason.
    Reject(u16, &'static str),
}

/// Incremental handshake reader: reuses the engine-http request parser and
/// only accepts the header-only GET of a WS opening handshake.
pub struct HandshakeReader {
    decoder: RequestDecoder,
}

impl Default for HandshakeReader {
    fn default() -> Self {
        Self::new()
    }
}

impl HandshakeReader {
    pub fn new() -> Self {
        Self {
            decoder: RequestDecoder::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> HandshakeOutcome {
        self.decoder.feed(bytes);
        match self.decoder.next_request() {
            ParseOutcome::NeedMore => HandshakeOutcome::NeedMore,
            ParseOutcome::Failed(status, reason) => HandshakeOutcome::Reject(status, reason),
            ParseOutcome::Complete { request, .. } => match validate(&request) {
                Ok(req) => HandshakeOutcome::Ok(req),
                Err(e) => HandshakeOutcome::Reject(reject_status(&e), reject_reason(&e)),
            },
        }
    }
}

fn reject_status(err: &HandshakeError) -> u16 {
    match err {
        HandshakeError::NotGet => 405,
        _ => 400,
    }
}

fn reject_reason(err: &HandshakeError) -> &'static str {
    match err {
        HandshakeError::Malformed(s) => s,
        HandshakeError::NotWebSocket => "upgrade to websocket required",
        HandshakeError::MissingKey => "missing sec-websocket-key",
        HandshakeError::UnsupportedVersion(_) => "unsupported sec-websocket-version",
        HandshakeError::NotGet => "websocket handshake requires GET",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6455_accept_key() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    fn raw_handshake(extra: &str) -> Vec<u8> {
        format!(
            "GET /chat HTTP/1.1\r\nHost: server.example.com\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13{extra}\r\n\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn accepts_valid_handshake_incrementally() {
        let bytes = raw_handshake("");
        let mut reader = HandshakeReader::new();
        assert!(matches!(reader.feed(&bytes[..10]), HandshakeOutcome::NeedMore));
        match reader.feed(&bytes[10..]) {
            HandshakeOutcome::Ok(req) => {
                assert_eq!(req.path, "/chat");
                assert_eq!(req.key, "dGhlIHNhbXBsZSBub25jZQ==");
            },
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_upgrade_headers() {
        let bytes = b"GET /chat HTTP/1.1\r\nHost: x\r\nSec-WebSocket-Key: abc\r\nSec-WebSocket-Version: 13\r\n\r\n";
        let mut reader = HandshakeReader::new();
        match reader.feed(bytes) {
            HandshakeOutcome::Reject(400, _) => {},
            other => panic!("expected reject, got {other:?}"),
        }
    }

    #[test]
    fn rejects_wrong_version() {
        let bytes = b"GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: abc\r\nSec-WebSocket-Version: 8\r\n\r\n";
        let mut reader = HandshakeReader::new();
        assert!(matches!(reader.feed(bytes), HandshakeOutcome::Reject(400, _)));
    }

    #[test]
    fn accept_response_contains_correct_key_and_headers() {
        let bytes = raw_handshake("");
        let mut reader = HandshakeReader::new();
        let req = match reader.feed(&bytes) {
            HandshakeOutcome::Ok(r) => r,
            _ => panic!("handshake failed"),
        };
        let resp = String::from_utf8(accept_response(&req, &[("X-Engine", "openoxide".into())])).unwrap();
        assert!(resp.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(resp.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
        assert!(resp.contains("X-Engine: openoxide\r\n"));
        assert!(resp.ends_with("\r\n\r\n"));
    }
}
