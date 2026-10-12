//! Request/response builders for HTTP/3 + WebTransport header blocks.
//!
//! Instead of hand-assembling `HeaderField` lists at every call site, build
//! typed requests/responses and convert to QPACK fields with `to_fields()`.
//!
//! ```
//! use engine_webtransport::WtRequest;
//!
//! let req = WtRequest::webtransport("localhost:4433", "/echo")
//!     .header("x-game-room", "rush");
//! let fields = req.to_fields();
//! ```

use qpack::HeaderField;

/// Header carrying the client-chosen session id on extended CONNECT.
pub const WT_SESSION_ID_HEADER: &str = "sec-webtransport-session-id";
/// Header advertised on the origin probe GET response.
pub const WT_HTTP3_DRAFT_HEADER: &str = "sec-webtransport-http3-draft";
/// Draft version browsers look for before attempting a WT connection.
pub const WT_HTTP3_DRAFT: &str = "draft02";
/// Extended CONNECT pseudo-header value identifying WebTransport.
pub const WT_PROTOCOL: &str = "webtransport";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    MissingPseudo(&'static str),
    BadUtf8,
}

/// A parsed or to-be-serialized HTTP/3 request header block.
///
/// Pseudo-headers are first-class fields; everything else lives in the
/// ordered `headers` list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WtRequest {
    method: String,
    scheme: String,
    authority: String,
    path: String,
    protocol: Option<String>,
    headers: Vec<(String, String)>,
}

impl WtRequest {
    /// Plain `GET` request (e.g. the origin probe).
    pub fn get(authority: impl Into<String>, path: impl Into<String>) -> Self {
        Self::new("GET", authority, path)
    }

    /// Extended CONNECT for a WebTransport session.
    pub fn webtransport(authority: impl Into<String>, path: impl Into<String>) -> Self {
        Self::new("CONNECT", authority, path).protocol(WT_PROTOCOL)
    }

    fn new(method: &str, authority: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.to_string(),
            scheme: "https".to_string(),
            authority: authority.into(),
            path: path.into(),
            protocol: None,
            headers: Vec::new(),
        }
    }

    pub fn protocol(mut self, p: impl Into<String>) -> Self {
        self.protocol = Some(p.into());
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn scheme(&self) -> &str {
        &self.scheme
    }
    pub fn authority(&self) -> &str {
        &self.authority
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn protocol_opt(&self) -> Option<&str> {
        self.protocol.as_deref()
    }
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Case-insensitive lookup over the non-pseudo headers.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The `sec-webtransport-session-id` value, when the client sent one.
    pub fn session_id_tag(&self) -> Option<&str> {
        self.header_value(WT_SESSION_ID_HEADER)
    }

    /// True when this is a WebTransport extended CONNECT.
    pub fn is_webtransport_connect(&self) -> bool {
        self.method == "CONNECT" && self.protocol.as_deref() == Some(WT_PROTOCOL)
    }

    /// Serialize into QPACK fields, pseudo-headers first (RFC 9114 §4.2).
    pub fn to_fields(&self) -> Vec<HeaderField> {
        let mut out = Vec::with_capacity(4 + self.headers.len());
        out.push(hf(b":method", &self.method));
        out.push(hf(b":scheme", &self.scheme));
        if self.method == "CONNECT" {
            if let Some(p) = &self.protocol {
                out.push(hf(b":protocol", p));
            }
        }
        if !self.authority.is_empty() {
            out.push(hf(b":authority", &self.authority));
        }
        out.push(hf(b":path", &self.path));
        for (k, v) in &self.headers {
            out.push(hf(k.as_bytes(), v));
        }
        out
    }

    /// Parse a decoded QPACK field list into a structured request.
    pub fn from_fields(fields: &[HeaderField]) -> Result<Self, HeaderError> {
        let mut req = WtRequest::default();
        for f in fields {
            let name = std::str::from_utf8(&f.name).map_err(|_| HeaderError::BadUtf8)?;
            let value = std::str::from_utf8(&f.value).map_err(|_| HeaderError::BadUtf8)?;
            match name {
                ":method" => req.method = value.to_string(),
                ":scheme" => req.scheme = value.to_string(),
                ":authority" => req.authority = value.to_string(),
                ":path" => req.path = value.to_string(),
                ":protocol" => req.protocol = Some(value.to_string()),
                ":status" => {}
                other => req
                    .headers
                    .push((other.to_string(), value.to_string())),
            }
        }
        if req.method.is_empty() {
            return Err(HeaderError::MissingPseudo(":method"));
        }
        Ok(req)
    }
}

/// A parsed or to-be-serialized HTTP/3 response header block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WtResponse {
    status: u16,
    headers: Vec<(String, String)>,
}

impl WtResponse {
    pub fn ok() -> Self {
        Self::status(200)
    }

    pub fn status(code: u16) -> Self {
        Self {
            status: code,
            headers: Vec::new(),
        }
    }

    /// 200 + `sec-webtransport-http3-draft: draft02`, the browser probe reply.
    pub fn http3_probe() -> Self {
        Self::ok().header(WT_HTTP3_DRAFT_HEADER, WT_HTTP3_DRAFT)
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn code(&self) -> u16 {
        self.status
    }
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn to_fields(&self) -> Vec<HeaderField> {
        let mut out = Vec::with_capacity(1 + self.headers.len());
        out.push(hf_owned(b":status", self.status.to_string()));
        for (k, v) in &self.headers {
            out.push(hf(k.as_bytes(), v));
        }
        out
    }

    pub fn from_fields(fields: &[HeaderField]) -> Result<Self, HeaderError> {
        let mut status = None;
        let mut headers = Vec::new();
        for f in fields {
            let name = std::str::from_utf8(&f.name).map_err(|_| HeaderError::BadUtf8)?;
            let value = std::str::from_utf8(&f.value).map_err(|_| HeaderError::BadUtf8)?;
            if name == ":status" {
                status = Some(value);
            } else {
                headers.push((name.to_string(), value.to_string()));
            }
        }
        Ok(Self {
            status: status
                .ok_or(HeaderError::MissingPseudo(":status"))?
                .parse()
                .map_err(|_| HeaderError::MissingPseudo(":status"))?,
            headers,
        })
    }
}

fn hf(name: &[u8], value: &str) -> HeaderField {
    HeaderField::new(name.to_vec(), value.as_bytes().to_vec())
}

fn hf_owned(name: &[u8], value: String) -> HeaderField {
    HeaderField::new(name.to_vec(), value.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webtransport_connect_roundtrips_through_qpack() {
        let req = WtRequest::webtransport("localhost:4433", "/echo")
            .header(WT_SESSION_ID_HEADER, "tag-42")
            .header("x-game-room", "rush");
        assert!(req.is_webtransport_connect());

        let mut block = bytes::BytesMut::new();
        qpack::encode_stateless(&mut block, req.to_fields().iter()).unwrap();

        let mut cur = block.freeze();
        let decoded = qpack::decode_stateless(&mut cur, u64::MAX).unwrap();
        let back = WtRequest::from_fields(&decoded.fields).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.session_id_tag(), Some("tag-42"));
        assert_eq!(back.header_value("X-Game-Room"), Some("rush"));
    }

    #[test]
    fn get_probe_has_scheme_not_protocol() {
        let req = WtRequest::get("example.com", "/");
        let fields = req.to_fields();
        let names: Vec<&[u8]> = fields.iter().map(|f| &*f.name).collect();
        assert!(names.contains(&&b":scheme"[..].as_ref()));
        assert!(!names.contains(&&b":protocol"[..].as_ref()));
        assert!(!req.is_webtransport_connect());
    }

    #[test]
    fn response_probe_reply_carries_draft_header() {
        let resp = WtResponse::http3_probe();
        let fields = resp.to_fields();
        assert_eq!(&*fields[0].name, b":status");
        assert_eq!(&*fields[0].value, b"200");
        let mut block = bytes::BytesMut::new();
        qpack::encode_stateless(&mut block, fields.iter()).unwrap();
        let mut cur = block.freeze();
        let d = qpack::decode_stateless(&mut cur, u64::MAX).unwrap();
        let back = WtResponse::from_fields(&d.fields).unwrap();
        assert_eq!(back, resp);
    }
}
