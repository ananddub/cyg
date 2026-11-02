use crate::client::{MAX_BODY_BYTES, MAX_HEAD_BYTES, Reply};
use crate::error::SidecarError;

mod chunked;
mod status;

#[cfg(test)]
mod tests;

use chunked::{Chunk, decode_chunked};
use status::{Framing, parse_head};

#[derive(Debug)]
pub(crate) enum Parse {
    Complete(Reply),
    Partial,
    Failed(SidecarError),
}

pub(crate) fn try_parse(buf: &[u8], ended: bool) -> Parse {
    let Some(head_end) = find_head_end(buf) else {
        if buf.len() >= MAX_HEAD_BYTES {
            return Parse::Failed(SidecarError::Malformed {
                reason: format!("header block exceeded {MAX_HEAD_BYTES} bytes"),
            });
        }
        return if ended {
            Parse::Failed(SidecarError::Malformed {
                reason: format!(
                    "truncated reply: {} bytes with no complete header block",
                    buf.len()
                ),
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

    if head.status == 204 || head.status == 304 {
        return Parse::Complete(Reply {
            status: head.status,
            reason: head.reason,
            body: Vec::new(),
        });
    }

    match head.framing() {
        Framing::Length(n) => {
            if n > MAX_BODY_BYTES {
                return Parse::Failed(SidecarError::Malformed {
                    reason: format!(
                        "sidecar promised a {n}-byte body, over our {MAX_BODY_BYTES}-byte cap"
                    ),
                });
            }
            if body.len() >= n {
                Parse::Complete(Reply {
                    status: head.status,
                    reason: head.reason,
                    body: body[..n].to_vec(),
                })
            } else if ended {
                Parse::Failed(SidecarError::Malformed {
                    reason: format!(
                        "truncated body: sidecar promised {n} bytes, sent {}",
                        body.len()
                    ),
                })
            } else {
                Parse::Partial
            }
        }
        Framing::Chunked => match decode_chunked(body) {
            Chunk::Complete(decoded) => Parse::Complete(Reply {
                status: head.status,
                reason: head.reason,
                body: decoded,
            }),
            Chunk::Bad(reason) => Parse::Failed(SidecarError::Malformed { reason }),
            Chunk::More => {
                if ended {
                    Parse::Failed(SidecarError::Malformed {
                        reason: "truncated chunked body".into(),
                    })
                } else {
                    Parse::Partial
                }
            }
        },
        Framing::UntilEof => {
            if ended {
                Parse::Complete(Reply {
                    status: head.status,
                    reason: head.reason,
                    body: body.to_vec(),
                })
            } else {
                Parse::Partial
            }
        }
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}
