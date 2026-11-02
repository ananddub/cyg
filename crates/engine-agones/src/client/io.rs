use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use super::http::{Parse, try_parse};
use super::{MAX_BODY_BYTES, MAX_HEAD_BYTES, READ_SLICE, Reply, SidecarConfig};
use crate::error::SidecarError;

pub(super) fn request(config: &SidecarConfig, method: &str, path: &str) -> Result<Reply, SidecarError> {
    let started = Instant::now();
    let deadline = started + config.timeout;
    let mut stream = connect(config, deadline)?;

    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nUser-Agent: cyg-agones/0.1\r\nAccept: */*\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        config.addr
    );
    let budget = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(1));
    stream
        .set_write_timeout(Some(budget))
        .map_err(|e| unreachable(config, format!("set_write_timeout failed: {e}")))?;
    if let Err(e) = stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.flush())
    {
        return Err(unreachable(
            config,
            format!("sending `{method} {path}` failed: {e}"),
        ));
    }

    read_reply(config, &mut stream, deadline)
}

fn connect(config: &SidecarConfig, deadline: Instant) -> Result<TcpStream, SidecarError> {
    let addrs: Vec<SocketAddr> = match config.addr.as_str().to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            return Err(unreachable(
                config,
                format!("cannot resolve `{}`: {e}", config.addr),
            ));
        }
    };
    if addrs.is_empty() {
        return Err(unreachable(
            config,
            format!("`{}` resolved to no address", config.addr),
        ));
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
        let slice = (deadline - now)
            .min(READ_SLICE)
            .max(Duration::from_millis(1));
        stream
            .set_read_timeout(Some(slice))
            .map_err(|e| unreachable(config, format!("set_read_timeout failed: {e}")))?;
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                return Err(unreachable(
                    config,
                    format!("reading the reply failed: {e}"),
                ));
            }
        }
    }

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
            reason: format!(
                "sidecar closed the connection after {} unreadable bytes",
                buf.len()
            ),
        }),
    }
}

fn unreachable(config: &SidecarConfig, reason: String) -> SidecarError {
    SidecarError::Unreachable {
        addr: config.addr.clone(),
        reason,
    }
}
