//! The loopback HTTP/1.1 client, hand-rolled on `std::net`.
//!
//! Why this is not a dependency: the sidecar speaks four POSTs and one GET over
//! loopback, from inside the pod. A full HTTP client would buy us a TLS stack we
//! cannot use and an async runtime we deliberately do not want on this thread.
//! What it must not buy us is *incorrectness*, so the framing is handled
//! properly: `Content-Length`, `Transfer-Encoding: chunked`, and a
//! `Connection: close` reply with neither all parse, and a reply that is
//! truncated, oversized or full of non-UTF-8 bytes becomes
//! [`SidecarError::Malformed`] instead of a panic.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::error::SidecarError;
use crate::state::{AgonesController, StateSnapshot};

/// Largest header block we will buffer before giving up on a reply.
const MAX_HEAD_BYTES: usize = 32 * 1024;
/// Largest reply body we will buffer. The sidecar sends a GameServer JSON; a
/// megabyte means something is not the sidecar on this port.
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Never block a read for longer than this, so `stop` flags and deadlines stay
/// responsive even when a peer dribbles bytes.
const READ_SLICE: Duration = Duration::from_millis(50);

/// Where the sidecar is and how long we wait for it.
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    /// `host:port` of the SDK sidecar, e.g. `127.0.0.1:9358`. IPv6 literals must
    /// be bracketed (`[::1]:9358`) — the same form `to_socket_addrs` expects.
    pub addr: String,
    /// Per-request budget covering connect, write and the complete reply. Keep it
    /// comfortably below the health interval: a request that outlives the
    /// interval makes the *next* ping late, and three late pings is an Unhealthy
    /// pod.
    pub timeout: Duration,
}

impl SidecarConfig {
    /// `host` and `port` as configured by `[control.agones]`.
    pub fn new(host: &str, port: u16, timeout: Duration) -> SidecarConfig {
        SidecarConfig { addr: format!("{host}:{port}"), timeout }
    }

    /// Loopback on `port` with a one-second budget — the usual pod shape.
    pub fn loopback(port: u16) -> SidecarConfig {
        SidecarConfig::new("127.0.0.1", port, Duration::from_secs(1))
    }

    pub fn with_timeout(mut self, timeout: Duration) -> SidecarConfig {
        self.timeout = timeout;
        self
    }
}

/// A parsed sidecar reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// HTTP status code.
    pub status: u16,
    /// Reason phrase from the status line (`"OK"`, `"Bad Request"`), `""` if absent.
    pub reason: String,
    /// Body bytes, exactly as framed. Empty for `204`/`304` and for bodies the
    /// sidecar omitted.
    pub body: Vec<u8>,
}

impl Reply {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text. Invalid UTF-8 becomes replacement characters instead of
    /// an error, because this feeds a log line and an error message.
    pub fn body_lossy(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// A sidecar client. Cheap to clone; every call opens its own short-lived
/// connection, which is what `Connection: close` asks for anyway.
#[derive(Debug, Clone)]
pub struct AgonesClient {
    config: SidecarConfig,
}

impl AgonesClient {
    pub fn new(config: SidecarConfig) -> AgonesClient {
        AgonesClient { config }
    }

    pub fn config(&self) -> &SidecarConfig {
        &self.config
    }

    /// Any method, any path, one connection.
    pub fn call(&self, method: &str, path: &str) -> Result<Reply, SidecarError> {
        request(&self.config, method, path)
    }

    /// A POST with no body. The four lifecycle transitions all look like this.
    /// A 2xx is success; anything else is [`SidecarError::Rejected`] with the
    /// sidecar's own explanation attached, because "you cannot go Ready from
    /// Shutdown" is information the operator needs.
    pub fn post(&self, path: &str) -> Result<Reply, SidecarError> {
        let reply = self.call("POST", path)?;
        if reply.is_success() {
            return Ok(reply);
        }
        Err(SidecarError::Rejected {
            status: reply.status,
            reason: reply.reason.clone(),
            body: truncate(&reply.body_lossy(), 200),
        })
    }

    /// `GET /state`, parsed. A non-2xx here is a rejection, not a mystery.
    pub fn get_state(&self) -> Result<StateSnapshot, SidecarError> {
        let reply = self.call("GET", "/state")?;
        if !reply.is_success() {
            return Err(SidecarError::Rejected {
                status: reply.status,
                reason: reply.reason.clone(),
                body: truncate(&reply.body_lossy(), 200),
            });
        }
        StateSnapshot::from_json(&reply.body_lossy())
    }
}

impl AgonesController for AgonesClient {
    fn mark_ready(&self) -> Result<(), SidecarError> {
        self.post("/ready").map(|_| ())
    }

    fn send_health_ping(&self) -> Result<(), SidecarError> {
        self.post("/health").map(|_| ())
    }

    fn mark_allocated(&self) -> Result<(), SidecarError> {
        self.post("/allocated").map(|_| ())
    }

    fn shutdown(&self) -> Result<(), SidecarError> {
        self.post("/shutdown").map(|_| ())
    }

    fn state(&self) -> Result<StateSnapshot, SidecarError> {
        self.get_state()
    }
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

fn request(config: &SidecarConfig, method: &str, path: &str) -> Result<Reply, SidecarError> {
    let started = Instant::now();
    let deadline = started + config.timeout;
    let mut stream = connect(config, deadline)?;

    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nUser-Agent: openoxide-agones/0.1\r\nAccept: */*\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        config.addr
    );
    let budget = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
    stream
        .set_write_timeout(Some(budget))
        .map_err(|e| unreachable(config, format!("set_write_timeout failed: {e}")))?;
    if let Err(e) = stream.write_all(head.as_bytes()).and_then(|_| stream.flush()) {
        return Err(unreachable(config, format!("sending `{method} {path}` failed: {e}")));
    }

    read_reply(config, &mut stream, deadline)
}

/// Try every resolved address in turn inside one shared deadline. A refused
/// connection is the common case (sidecar container not started yet) and must
/// cost one dial, not a retry storm.
fn connect(config: &SidecarConfig, deadline: Instant) -> Result<TcpStream, SidecarError> {
    let addrs: Vec<SocketAddr> = match config.addr.as_str().to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => return Err(unreachable(config, format!("cannot resolve `{}`: {e}", config.addr))),
    };
    if addrs.is_empty() {
        return Err(unreachable(config, format!("`{}` resolved to no address", config.addr)));
    }
    let mut last = String::from("no address attempted");
    for addr in addrs {
        let budget = deadline.saturating_duration_since(Instant::now());
        if budget.is_zero() {
            return Err(SidecarError::Timeout {
                addr: config.addr.clone(),
                after_ms: config.timeout.as_millis() as u64,
            });
        }
        match TcpStream::connect_timeout(&addr, budget.max(Duration::from_millis(1))) {
            Ok(stream) => {
                // Loopback, four tiny writes: Nagle would only add latency.
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    Err(unreachable(config, last))
}

fn read_reply(
    config: &SidecarConfig,
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<Reply, SidecarError> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);
    let mut tmp = [0u8; 4096];
    let mut timed_out = false;

    loop {
        match try_parse(&buf, false) {
            Parse::Complete(reply) => return Ok(reply),
            Parse::Failed(e) => return Err(e),
            Parse::Partial => {}
        }
        if buf.len() > MAX_HEAD_BYTES + MAX_BODY_BYTES {
            return Err(SidecarError::Malformed {
                reason: format!("reply exceeded {} bytes", MAX_HEAD_BYTES + MAX_BODY_BYTES),
            });
        }
        let now = Instant::now();
        if now >= deadline {
            timed_out = true;
            break;
        }
        let slice = (deadline - now).min(READ_SLICE).max(Duration::from_millis(1));
        stream
            .set_read_timeout(Some(slice))
            .map_err(|e| unreachable(config, format!("set_read_timeout failed: {e}")))?;
        match stream.read(&mut tmp) {
            Ok(0) => break, // peer closed: everything close-delimited is finished
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(unreachable(config, format!("reading the reply failed: {e}"))),
        }
    }

    // EOF or deadline. A deadline is a liveness verdict and outranks whatever the
    // partial bytes would have complained about: "no complete reply in time" is the
    // thing an operator can act on, and it is retryable.
    if timed_out {
        return Err(SidecarError::Timeout {
            addr: config.addr.clone(),
            after_ms: config.timeout.as_millis() as u64,
        });
    }
    match try_parse(&buf, true) {
        Parse::Complete(reply) => Ok(reply),
        Parse::Failed(e) => Err(e),
        Parse::Partial => Err(SidecarError::Malformed {
            reason: format!("sidecar closed the connection after {} unreadable bytes", buf.len()),
        }),
    }
}

/// What the bytes so far are worth. `ended` means "no more bytes are coming":
/// the peer closed, or we ran out of budget.
#[derive(Debug)]
enum Parse {
    Complete(Reply),
    Partial,
    Failed(SidecarError),
}

fn try_parse(buf: &[u8], ended: bool) -> Parse {
    let Some(head_end) = find_head_end(buf) else {
        if buf.len() >= MAX_HEAD_BYTES {
            return Parse::Failed(SidecarError::Malformed {
                reason: format!("header block exceeded {MAX_HEAD_BYTES} bytes"),
            });
        }
        return if ended {
            Parse::Failed(SidecarError::Malformed {
                reason: format!("truncated reply: {} bytes with no complete header block", buf.len()),
            })
        } else {
            Parse::Partial
        };
    };

    let head = match parse_head(&buf[..head_end]) {
        Ok(head) => head,
        Err(e) => return Parse::Failed(e),
    };
    let body = &buf[head_end + 4..];

    // A reply that must not carry a body is complete the moment its headers are.
    if head.status == 204 || head.status == 304 {
        return Parse::Complete(Reply { status: head.status, reason: head.reason, body: Vec::new() });
    }

    match head.framing() {
        Framing::Length(n) => {
            if n > MAX_BODY_BYTES {
                return Parse::Failed(SidecarError::Malformed {
                    reason: format!("sidecar promised a {n}-byte body, over our {MAX_BODY_BYTES}-byte cap"),
                });
            }
            if body.len() >= n {
                Parse::Complete(Reply { status: head.status, reason: head.reason, body: body[..n].to_vec() })
            } else if ended {
                Parse::Failed(SidecarError::Malformed {
                    reason: format!("truncated body: sidecar promised {n} bytes, sent {}", body.len()),
                })
            } else {
                Parse::Partial
            }
        }
        Framing::Chunked => match decode_chunked(body) {
            Chunk::Complete(decoded) => {
                Parse::Complete(Reply { status: head.status, reason: head.reason, body: decoded })
            }
            Chunk::Bad(reason) => Parse::Failed(SidecarError::Malformed { reason }),
            Chunk::More => {
                if ended {
                    Parse::Failed(SidecarError::Malformed { reason: "truncated chunked body".into() })
                } else {
                    Parse::Partial
                }
            }
        },
        // `Connection: close` with no length: the body is whatever arrives.
        Framing::UntilEof => {
            if ended {
                Parse::Complete(Reply { status: head.status, reason: head.reason, body: body.to_vec() })
            } else {
                Parse::Partial
            }
        }
    }
}

/// Index of the `\r\n\r\n` that ends the header block.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

struct Head {
    status: u16,
    reason: String,
    content_length: Option<usize>,
    chunked: bool,
}

enum Framing {
    Length(usize),
    Chunked,
    UntilEof,
}

impl Head {
    fn framing(&self) -> Framing {
        if self.chunked {
            Framing::Chunked
        } else if let Some(n) = self.content_length {
            Framing::Length(n)
        } else {
            Framing::UntilEof
        }
    }
}

/// Parse the status line and headers. Header text is decoded lossily: a peer
/// that emits raw bytes in a reason phrase must not take the health loop down.
fn parse_head(head: &[u8]) -> Result<Head, SidecarError> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("").trim();
    let mut fields = status_line.splitn(3, ' ');
    let version = fields.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(SidecarError::Malformed {
            reason: format!("no HTTP status line (starts with {:?})", truncate(status_line, 40)),
        });
    }
    let Some(status) = fields.next().and_then(|v| v.trim().parse::<u16>().ok()) else {
        return Err(SidecarError::Malformed {
            reason: format!("status line has no numeric code: {:?}", truncate(status_line, 60)),
        });
    };
    let reason = fields.next().unwrap_or("").trim().to_string();

    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                let parsed: Option<usize> = value.trim().parse().ok();
                match (content_length, parsed) {
                    (None, Some(n)) => content_length = Some(n),
                    // Conflicting lengths are a broken proxy, not a choice for us.
                    (Some(prev), Some(n)) if prev != n => {
                        return Err(SidecarError::Malformed {
                            reason: format!("conflicting Content-Length values ({prev} and {n})"),
                        });
                    }
                    (None, None) => {
                        return Err(SidecarError::Malformed {
                            reason: format!("unparseable Content-Length {:?}", truncate(value.trim(), 20)),
                        });
                    }
                    _ => {}
                }
            }
            "transfer-encoding"
                if value.to_ascii_lowercase().contains("chunked") => {
                chunked = true;
            }
            _ => {}
        }
    }

    Ok(Head { status, reason, content_length, chunked })
}

enum Chunk {
    Complete(Vec<u8>),
    More,
    Bad(String),
}

/// Decode `Transfer-Encoding: chunked` (grpc-gateway does use it). Chunk
/// extensions after `;` are ignored; trailers after the last chunk are dropped.
fn decode_chunked(body: &[u8]) -> Chunk {
    let mut out: Vec<u8> = Vec::new();
    let mut rest = body;
    loop {
        let Some(line_end) = find_crlf(rest) else { return Chunk::More };
        let line = match std::str::from_utf8(&rest[..line_end]) {
            Ok(line) => line,
            Err(_) => return Chunk::Bad("chunk size line was not UTF-8".into()),
        };
        let size_field = line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_field, 16) else {
            return Chunk::Bad(format!("chunk size {size_field:?} is not hexadecimal"));
        };
        if size == 0 {
            return Chunk::Complete(out);
        }
        let data_start = line_end + 2;
        if rest.len() < data_start + size + 2 {
            return Chunk::More;
        }
        if &rest[data_start + size..data_start + size + 2] != b"\r\n" {
            return Chunk::Bad("chunk data was not followed by CRLF".into());
        }
        out.extend_from_slice(&rest[data_start..data_start + size]);
        if out.len() > MAX_BODY_BYTES {
            return Chunk::Bad(format!("chunked body exceeded the {MAX_BODY_BYTES}-byte cap"));
        }
        rest = &rest[data_start + size + 2..];
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn unreachable(config: &SidecarConfig, reason: String) -> SidecarError {
    SidecarError::Unreachable { addr: config.addr.clone(), reason }
}

/// Byte-safe truncation for log and error text: cuts at a char boundary so the
/// result is always valid UTF-8.
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    fn reply_bytes(body: &[u8]) -> Reply {
        Reply { status: 200, reason: "OK".into(), body: body.to_vec() }
    }

    #[test]
    fn parses_content_length_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        match try_parse(raw, false) {
            Parse::Complete(r) => assert_eq!(r.body, b"hello"),
            other => panic!("expected a complete reply, got {other:?}"),
        }
        // Extra bytes past the promised length are not part of the body.
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhelloTRAILER";
        match try_parse(raw, false) {
            Parse::Complete(r) => assert_eq!(r.body, b"hello"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_close_delimited_body() {
        let raw = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"state\":\"Ready\"}";
        match try_parse(raw, false) {
            Parse::Partial => {}
            other => panic!("without a length the body is not finished yet: {other:?}"),
        }
        match try_parse(raw, true) {
            Parse::Complete(r) => assert_eq!(r.body_lossy(), "{\"state\":\"Ready\"}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_chunked_body() {
        let raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nfoo\r\n6\r\nbarbaz\r\n0\r\n\r\n";
        match try_parse(raw, false) {
            Parse::Complete(r) => assert_eq!(r.body_lossy(), "foobarbaz"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_chunk_size_that_does_not_match_its_data_is_malformed() {
        // Five declared, six delivered: the framing is a contract, not a hint.
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nbarbaz\r\n0\r\n\r\n";
        match try_parse(raw, false) {
            Parse::Failed(SidecarError::Malformed { reason }) => {
                assert!(reason.contains("CRLF"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_non_hexadecimal_chunk_size_is_malformed() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nfoo\r\n";
        assert!(matches!(try_parse(raw, false), Parse::Failed(SidecarError::Malformed { .. })));
    }

    #[test]
    fn chunked_with_missing_terminator_is_not_complete() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nfoo";
        assert!(matches!(try_parse(raw, false), Parse::Partial));
        assert!(matches!(try_parse(raw, true), Parse::Failed(SidecarError::Malformed { .. })));
    }

    #[test]
    fn truncated_content_length_is_malformed_not_panic() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{\"st";
        match try_parse(raw, true) {
            Parse::Failed(SidecarError::Malformed { reason }) => {
                assert!(reason.contains("promised 100"), "{reason}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn no_header_block_at_eof_is_malformed() {
        let raw = b"HTTP/1.1 200 OK\r\nCont";
        assert!(matches!(try_parse(raw, false), Parse::Partial));
        assert!(matches!(try_parse(raw, true), Parse::Failed(SidecarError::Malformed { .. })));
    }

    #[test]
    fn non_utf8_headers_still_parse() {
        // A reason phrase containing raw bytes must not break status detection.
        let raw = b"HTTP/1.1 500 \xfe\xff\r\nContent-Length: 1\r\n\r\nx";
        match try_parse(raw, false) {
            Parse::Complete(r) => assert_eq!(r.status, 500),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_or_garbage_status_line_is_malformed() {
        let raw = b"not http at all\r\n\r\n";
        assert!(matches!(try_parse(raw, true), Parse::Failed(SidecarError::Malformed { .. })));
    }

    #[test]
    fn no_content_length_means_until_eof_and_204_is_immediate() {
        let raw = b"HTTP/1.1 204 No Content\r\n\r\n";
        assert!(matches!(try_parse(raw, false), Parse::Complete(r) if r.body.is_empty()));
    }

    #[test]
    fn conflicting_content_lengths_are_malformed() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 9\r\n\r\nhello";
        assert!(matches!(try_parse(raw, false), Parse::Failed(SidecarError::Malformed { .. })));
    }

    #[test]
    fn reply_success_and_lossy_body() {
        assert!(reply_bytes(b"").is_success());
        let r = Reply { status: 400, reason: "Bad Request".into(), body: vec![0xff, b'x'] };
        assert!(!r.is_success());
        assert!(r.body_lossy().contains('x'));
    }

    #[test]
    fn truncation_never_splits_a_char() {
        assert_eq!(truncate("héllo", 1), "h…");
        assert_eq!(truncate("short", 99), "short");
    }

    #[test]
    fn config_builds_host_port_addr() {
        let cfg = SidecarConfig::new("127.0.0.1", 9358, Duration::from_millis(500));
        assert_eq!(cfg.addr, "127.0.0.1:9358");
        assert_eq!(cfg.clone().with_timeout(Duration::from_secs(2)).timeout, Duration::from_secs(2));
        assert_eq!(SidecarConfig::loopback(9358).addr, "127.0.0.1:9358");
    }
}
