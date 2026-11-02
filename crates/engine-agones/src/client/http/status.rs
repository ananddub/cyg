use crate::client::truncate;
use crate::error::SidecarError;

pub(super) struct Head {
    pub(super) status: u16,
    pub(super) reason: String,
    content_length: Option<usize>,
    chunked: bool,
}

pub(super) enum Framing {
    Length(usize),
    Chunked,
    UntilEof,
}

impl Head {
    pub(super) fn framing(&self) -> Framing {
        if self.chunked {
            Framing::Chunked
        } else if let Some(n) = self.content_length {
            Framing::Length(n)
        } else {
            Framing::UntilEof
        }
    }
}

pub(super) fn parse_head(head: &[u8]) -> Result<Head, SidecarError> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("").trim();
    let mut fields = status_line.splitn(3, ' ');
    let version = fields.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(SidecarError::Malformed {
            reason: format!(
                "no HTTP status line (starts with {:?})",
                truncate(status_line, 40)
            ),
        });
    }
    let Some(status) = fields.next().and_then(|v| v.trim().parse::<u16>().ok()) else {
        return Err(SidecarError::Malformed {
            reason: format!(
                "status line has no numeric code: {:?}",
                truncate(status_line, 60)
            ),
        });
    };
    let reason = fields.next().unwrap_or("").trim().to_string();

    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                let parsed: Option<usize> = value.trim().parse().ok();
                match (content_length, parsed) {
                    (None, Some(n)) => content_length = Some(n),
                    (Some(prev), Some(n)) if prev != n => {
                        return Err(SidecarError::Malformed {
                            reason: format!("conflicting Content-Length values ({prev} and {n})"),
                        });
                    }
                    (None, None) => {
                        return Err(SidecarError::Malformed {
                            reason: format!(
                                "unparseable Content-Length {:?}",
                                truncate(value.trim(), 20)
                            ),
                        });
                    }
                    _ => {}
                }
            }
            "transfer-encoding" if value.to_ascii_lowercase().contains("chunked") => {
                chunked = true;
            }
            _ => {}
        }
    }

    Ok(Head {
        status,
        reason,
        content_length,
        chunked,
    })
}
