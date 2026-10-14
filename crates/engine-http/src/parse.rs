//! # Incremental HTTP/1.1 Request Parser
//!
//! A zero-dependency push parser: the backend feeds it socket bytes as they
//! arrive and it reports whether a full request is ready, needs more bytes,
//! or the stream is broken. Bodies are read via `Content-Length` only
//! (chunked encoding is intentionally unsupported for the game API surface).

use std::collections::HashMap;

use crate::types::{Headers, Method, Request};

/// Result of feeding bytes into a [`RequestDecoder`].
pub enum ParseOutcome {
    /// Need more bytes before a request can be produced.
    NeedMore,
    /// A complete request was decoded; `keep_alive` carries the connection
    /// reuse intent negotiated from the request head.
    Complete { request: Request, keep_alive: bool },
    /// The stream is malformed or a limit was exceeded. Carries an HTTP
    /// status to reply with before closing the connection.
    Failed(u16, &'static str),
}

/// Per-connection incremental decoder holding the receive buffer and the
/// parsed request head until the body finishes arriving.
pub struct RequestDecoder {
    buf: Vec<u8>,
    max_header_bytes: usize,
    max_body_bytes: usize,
    pending: Option<PendingHead>,
}

struct PendingHead {
    method: Method,
    target: String,
    headers: Headers,
    /// Index in `buf` where the body starts (after the blank line).
    body_start: usize,
    content_length: usize,
    keep_alive: bool,
}

impl RequestDecoder {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1024),
            max_header_bytes: 8 * 1024,
            max_body_bytes: 64 * 1024,
            pending: None,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Attempt to decode the next complete request from the buffer.
    pub fn next_request(&mut self) -> ParseOutcome {
        if let Some(head) = self.pending.take() {
            if self.buf.len() < head.body_start + head.content_length {
                self.pending = Some(head);
                return ParseOutcome::NeedMore;
            }
            let end = head.body_start + head.content_length;
            let body = self.take_body(head.body_start, end);
            return Self::finalize(head, body);
        }

        let header_end = match find_double_crlf(&self.buf) {
            Some(pos) => pos,
            None => {
                if self.buf.len() > self.max_header_bytes {
                    return ParseOutcome::Failed(431, "header too large");
                }
                return ParseOutcome::NeedMore;
            }
        };

        let head = match Self::parse_head(&self.buf, header_end, self.max_body_bytes) {
            Ok(head) => head,
            Err((status, reason)) => return ParseOutcome::Failed(status, reason),
        };

        let body_start = head.body_start;
        let content_length = head.content_length;
        let keep_alive = head.keep_alive;

        if content_length == 0 {
            self.consume_front(body_start);
            return Self::finalize(head, Vec::new());
        }

        if self.buf.len() < body_start + content_length {
            self.pending = Some(head);
            return ParseOutcome::NeedMore;
        }

        let end = body_start + content_length;
        let body = self.take_body(body_start, end);
        Self::finalize_pending(head, body, keep_alive)
    }

    /// Copy the body out and drop the consumed head+body from the front.
    /// The common case (one request per read) avoids the memmove entirely.
    fn take_body(&mut self, body_start: usize, end: usize) -> Vec<u8> {
        let body = self.buf[body_start..end].to_vec();
        self.consume_front(end);
        body
    }

    fn consume_front(&mut self, end: usize) {
        if end >= self.buf.len() {
            self.buf.clear();
        } else {
            self.buf.drain(0..end);
        }
    }

    fn parse_head(
        buf: &[u8],
        header_end: usize,
        max_body_bytes: usize,
    ) -> Result<PendingHead, (u16, &'static str)> {
        let head_bytes = &buf[..header_end];
        let text =
            std::str::from_utf8(head_bytes).map_err(|_| (400u16, "non-utf8 request head"))?;
        let mut lines = text.split("\r\n");

        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split(' ');
        let (Some(method_tok), Some(target), Some(version)) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err((400, "malformed request line"));
        };
        if !version.starts_with("HTTP/1.") {
            return Err((505, "unsupported HTTP version"));
        }
        let Some(method) = Method::from_str_token(method_tok) else {
            return Err((400, "unsupported method"));
        };

        let mut headers = Headers::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let Some((name, value)) = line.split_once(':') else {
                return Err((400, "malformed header line"));
            };
            headers.insert(name.trim(), value.trim());
        }

        let content_length = match headers.get("content-length") {
            Some(v) => v
                .trim()
                .parse::<usize>()
                .map_err(|_| (400u16, "bad content-length"))?,
            None => 0,
        };
        if content_length > max_body_bytes {
            return Err((413, "body too large"));
        }

        let connection = headers.get("connection").unwrap_or("");
        let keep_alive = if version == "HTTP/1.1" {
            !contains_ignore_ascii(connection, "close")
        } else {
            contains_ignore_ascii(connection, "keep-alive")
        };

        Ok(PendingHead {
            method,
            target: target.to_string(),
            headers,
            body_start: header_end + 4,
            content_length,
            keep_alive,
        })
    }

    fn finalize(head: PendingHead, body: Vec<u8>) -> ParseOutcome {
        let keep_alive = head.keep_alive;
        Self::finalize_pending(head, body, keep_alive)
    }

    fn finalize_pending(head: PendingHead, body: Vec<u8>, keep_alive: bool) -> ParseOutcome {
        let (path, raw_query) = split_target(&head.target);
        let path = path.to_string();
        let query = parse_query(raw_query);
        let request = Request {
            method: head.method,
            target: head.target,
            path,
            query,
            headers: head.headers,
            body,
            params: HashMap::new(),
        };
        ParseOutcome::Complete {
            request,
            keep_alive,
        }
    }
}

impl Default for RequestDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the position of the leading `\r` in the first `\r\n\r\n`.
fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Case-insensitive substring test on ASCII data without allocating.
fn contains_ignore_ascii(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() {
        return true;
    }
    if h.len() < n.len() {
        return false;
    }
    let lower = |b: u8| b.to_ascii_lowercase();
    h.windows(n.len()).any(|w| {
        w.iter()
            .zip(n.iter())
            .all(|(a, b)| lower(*a) == lower(*b))
    })
}

/// Split a request target into `/path` and the raw query string, both borrowed.
pub fn split_target(target: &str) -> (&str, &str) {
    match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    }
}

/// Decode an `application/x-www-form-urlencoded` query string into pairs.
pub fn parse_query(raw: &str) -> HashMap<String, String> {
    if raw.is_empty() {
        return HashMap::new();
    }
    let mut map = HashMap::new();
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(percent_decode(k).into_owned(), percent_decode(v).into_owned());
    }
    map
}

/// Minimal percent-decoding sufficient for query/path tokens. Borrows the
/// input when nothing needs decoding, which is the common case.
pub fn percent_decode(s: &str) -> std::borrow::Cow<'_, str> {
    let bytes = s.as_bytes();
    if !bytes.iter().any(|&b| b == b'%' || b == b'+') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                let byte = u8::from_str_radix(hex, 16).unwrap_or(b'%');
                out.push(byte);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    std::borrow::Cow::Owned(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Method;

    fn feed_and_parse(raw: &[u8]) -> ParseOutcome {
        let mut decoder = RequestDecoder::new();
        decoder.feed(raw);
        decoder.next_request()
    }

    #[test]
    fn parses_simple_get() {
        let out = feed_and_parse(b"GET /health HTTP/1.1\r\nHost: a\r\n\r\n");
        match out {
            ParseOutcome::Complete { request, keep_alive } => {
                assert_eq!(request.method, Method::Get);
                assert_eq!(request.path, "/health");
                assert!(keep_alive);
                assert!(request.body.is_empty());
            }
            _ => panic!("expected complete request"),
        }
    }

    #[test]
    fn splits_query_params() {
        let out = feed_and_parse(b"GET /rooms?slot=3&tag=a%20b HTTP/1.1\r\n\r\n");
        match out {
            ParseOutcome::Complete { request, .. } => {
                assert_eq!(request.path, "/rooms");
                assert_eq!(request.query_param("slot"), Some("3"));
                assert_eq!(request.query_param("tag"), Some("a b"));
            }
            _ => panic!("expected complete request"),
        }
    }

    #[test]
    fn parses_content_length_body() {
        let out = feed_and_parse(
            b"POST /echo HTTP/1.1\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
        );
        match out {
            ParseOutcome::Complete { request, keep_alive } => {
                assert_eq!(request.method, Method::Post);
                assert_eq!(request.body_str(), Some("hello"));
                assert!(!keep_alive);
            }
            _ => panic!("expected complete request"),
        }
    }

    #[test]
    fn need_more_until_headers_complete() {
        let mut decoder = RequestDecoder::new();
        decoder.feed(b"GET /x HTTP/1.1\r\nHost:");
        assert!(matches!(decoder.next_request(), ParseOutcome::NeedMore));
        decoder.feed(b" a\r\n\r\n");
        assert!(matches!(decoder.next_request(), ParseOutcome::Complete { .. }));
    }

    #[test]
    fn need_more_until_body_complete() {
        let mut decoder = RequestDecoder::new();
        decoder.feed(b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc");
        assert!(matches!(decoder.next_request(), ParseOutcome::NeedMore));
        decoder.feed(b"defghij");
        match decoder.next_request() {
            ParseOutcome::Complete { request, .. } => {
                assert_eq!(request.body_str(), Some("abcdefghij"));
            }
            _ => panic!("expected complete"),
        }
    }

    #[test]
    fn rejects_bad_content_length() {
        let out = feed_and_parse(b"POST /x HTTP/1.1\r\nContent-Length: abc\r\n\r\n");
        match out {
            ParseOutcome::Failed(400, _) => {}
            _ => panic!("expected 400"),
        }
    }

    #[test]
    fn rejects_oversized_body() {
        let out = feed_and_parse(b"POST /x HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n");
        match out {
            ParseOutcome::Failed(413, _) => {}
            _ => panic!("expected 413"),
        }
    }

    #[test]
    fn second_request_decoded_after_first_consumed() {
        let mut decoder = RequestDecoder::new();
        decoder.feed(b"GET /a HTTP/1.1\r\n\r\nGET /b HTTP/1.1\r\nConnection: close\r\n\r\n");
        match decoder.next_request() {
            ParseOutcome::Complete { request, keep_alive } => {
                assert_eq!(request.path, "/a");
                assert!(keep_alive);
            }
            _ => panic!("expected first request"),
        }
        match decoder.next_request() {
            ParseOutcome::Complete { request, .. } => {
                assert_eq!(request.path, "/b");
            }
            _ => panic!("expected second request"),
        }
    }
}
